//! Staging a map: the work of drawing it that does not depend on the window, in steps (the tiles
//! and their box, the objects, the scene, the tank extras, the ground pictures, the other
//! pictures in paint order).

use super::*;

/// The objects of a staging: what the scene is made of, what each stands for in the zoo (by
/// index in `list`), and how it is drawn.
struct Objects<'a> {
    list: Vec<Object>,
    metas: Vec<Meta<'a>>,
    owners: Vec<Option<pick::Hit>>,
    /// The index of the first object that is not the zoo's, a ghost or a tank's own: the ripples
    /// and the bubbles.
    transient: usize,
}

/// The tiles with their elevation kept in the range the scene handles.
fn clamped(zoo: &Zoo) -> Vec<Tile> {
    zoo.tiles
        .iter()
        .map(|t| Tile {
            elevation: t
                .elevation
                .clamp(-order::MAX_ELEVATION, order::MAX_ELEVATION),
            ..*t
        })
        .collect()
}

/// The box of every tile seen from `rotation`, refused when it is huge: staging costs memory in
/// proportion to its tiles.
fn tiles_box(tiles: &[Tile], width: usize, rotation: u8) -> Result<[i64; 4], RenderError> {
    let mut tile_bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
    for (t, tile) in tiles.iter().enumerate() {
        let (ox, oy) = origin(tile, (t % width) as i32, (t / width) as i32, rotation);
        let (x, y) = (i64::from(ox), i64::from(oy));
        tile_bounds = union(tile_bounds, [x, y + 16, x + 64, y + 49]);
    }
    check_size(&tile_bounds)?;
    Ok(tile_bounds)
}

impl<'a> MapRenderer<'a> {
    /// As [`MapRenderer::stage`], with the transient items of the map view: they are in the paint
    /// order but not in the zoo. The marks of `extra` give a tile (an index into the tiles) the
    /// highlight sprite in their colour, after its ground and grid lines (map drawing 7); a tile
    /// that has a mark takes the grid lines off the two tiles behind it.
    pub fn stage_with(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
        no_units: bool,
        extra: &Extra,
    ) -> Result<Staged, RenderError> {
        let marks: HashMap<usize, [u8; 3]> =
            extra.marks.iter().map(|m| (m.tile, m.color)).collect();
        let rotation = rotation % 4;
        let (width, height) = (zoo.width as usize, zoo.height as usize);
        let mut summary = Summary {
            width,
            height,
            ..Summary::default()
        };
        if width == 0 || height == 0 {
            return Err(RenderError("the map has no tile".into()));
        }
        let tiles = clamped(zoo);
        let tile_bounds = tiles_box(&tiles, width, rotation)?;
        let mut objects = self.entity_objects(zoo, no_units, extra, &mut summary);
        self.ghost_objects(extra, &mut objects);
        let list = std::mem::take(&mut objects.list);
        let mut scene = Scene::new(width, height, rotation, &tiles, list);
        set_tanks(&mut scene, zoo, &objects.metas);
        let looks: Vec<_> = zoo
            .tanks
            .iter()
            .map(|t| self.look(t.salt, t.purity))
            .collect();
        self.transients(&mut scene, zoo, extra, &looks, tiles.len(), &mut objects);
        let grounds = self.grounds_for(&scene, marks, width, rotation);
        let (items, own, hits) =
            self.place_items(&scene, &objects, &grounds, &looks, extra, &mut summary);
        summary.objects_skipped += (0..objects.metas.len())
            .filter(|&o| !scene.is_placed(o))
            .count();

        self.grounds = Some(Rc::clone(&grounds));
        let mut staged = Staged {
            rotation,
            grounds,
            items,
            own,
            hits,
            tile_bounds,
            bounds: tile_bounds,
            summary,
        };
        // The box of every tile and every picture.
        staged.bounds = staged
            .pictures()
            .filter(|(p, _)| !p.grid)
            .fold(tile_bounds, |all, (_, b)| union(all, *b));
        Ok(staged)
    }

    /// The objects the original keeps: the entities whose type it knows.
    fn entity_objects<'z>(
        &self,
        zoo: &'z Zoo,
        no_units: bool,
        extra: &'z Extra,
        summary: &mut Summary,
    ) -> Objects<'z>
    where
        'a: 'z,
    {
        let catalogue = self.catalogue;
        let shown: HashMap<u32, &Shown> = extra.shown.iter().map(|s| (s.id, s)).collect();
        let mut objects = Objects {
            list: Vec::new(),
            metas: Vec::new(),
            owners: Vec::new(),
            transient: 0,
        };
        for (position, entity) in zoo.entities.iter().enumerate() {
            let Some(c) = &entity.common else {
                summary.objects_skipped += 1;
                continue;
            };
            let kind = Kind::of_class(&entity.class);
            if no_units && matches!(kind, Kind::Unit | Kind::Overlay) {
                continue;
            }
            if extra.hidden.contains(&c.id) {
                continue;
            }
            let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
            let Some(ty) = catalogue.find(
                &entity.class,
                &bytes(&entity.type_name),
                &bytes(&entity.subtype),
            ) else {
                summary.objects_missing += 1;
                continue;
            };
            let [fx, fy, _] = ty.footprint;
            objects.list.push(Object {
                kind,
                x: c.x,
                y: c.y,
                z: c.z.clamp(-4096, 4096),
                facing: c.rotation % 8,
                footprint: (fx, fy),
                late: draws_late(ty),
                height: ty.height,
            });
            objects.owners.push(Some(pick::Hit {
                id: c.id,
                threshold: pick::threshold(ty),
            }));
            objects.metas.push(Meta {
                ty,
                colors: &c.colors,
                life: (c.fence.map(|f| f.life)).or_else(|| {
                    c.filter
                        .map(|f| i32::try_from(f.health).unwrap_or(i32::MAX))
                }),
                off: extra.filters.iter().any(|l| l.id == c.id && l.off),
                bubbles: None,
                mode: c.movement_mode,
                portal: c.fence.is_some_and(|f| f.portal),
                food: c.food,
                tint: extra
                    .tinted
                    .iter()
                    .find(|(e, _)| *e == position)
                    .map(|(_, tint)| *tint),
                wall: 0,
                shown: shown
                    .get(&c.id)
                    .map(|s| (s.animation.as_str(), s.frame as usize)),
            });
        }
        objects
    }

    /// The pending objects of the map view, after the entities, owned by none.
    fn ghost_objects<'z>(&self, extra: &'z Extra, objects: &mut Objects<'z>)
    where
        'a: 'z,
    {
        let catalogue = self.catalogue;
        for ghost in &extra.ghosts {
            let Some(ty) = catalogue.types.get(ghost.ty) else {
                continue;
            };
            objects.list.push(Object {
                kind: Kind::Overlay,
                x: ghost.x,
                y: ghost.y,
                z: ghost.z.clamp(-4096, 4096),
                facing: ghost.facing % 8,
                footprint: (ty.footprint[0], ty.footprint[1]),
                late: draws_late(ty),
                height: ty.height,
            });
            objects.owners.push(None);
            objects.metas.push(Meta {
                ty,
                colors: &[],
                life: None,
                off: false,
                bubbles: None,
                mode: 0,
                portal: false,
                food: 0,
                tint: Some(ghost.tint),
                wall: ghost.wall.min(order::MAX_ELEVATION as u32) as i32,
                shown: None,
            });
        }
    }

    /// The ripples against the active walls of the tanks and the bubbles of the filters, which
    /// are in the scene and the metas but in no entity of the zoo.
    fn transients<'z>(
        &self,
        scene: &mut Scene,
        zoo: &Zoo,
        extra: &Extra,
        looks: &[tank::Look],
        tiles: usize,
        objects: &mut Objects<'z>,
    ) where
        'a: 'z,
    {
        let catalogue = self.catalogue;
        objects.transient = objects.metas.len();
        let transient = |ty, bubbles| Meta {
            ty,
            colors: &[],
            life: None,
            off: false,
            bubbles,
            mode: 0,
            portal: false,
            food: 0,
            tint: None,
            wall: 0,
            shown: None,
        };
        // The game puts a ripple on the clean water against every active wall of a tank.
        let ripple = catalogue.find("objects", b"other", b"tankripl");
        for ty in ripple.iter().flat_map(|&ty| {
            let n = add_ripples(scene, looks, tiles);
            std::iter::repeat_n(ty, n)
        }) {
            objects.metas.push(transient(ty, None));
        }
        // So is the column of bubbles a filter's cycle leaves on the tank side of its wall.
        for (ty, tile, age, obj) in bubbles(scene, zoo, catalogue, extra) {
            scene.add_transient(tile, obj);
            objects.metas.push(transient(ty, Some(age)));
        }
    }

    /// The pictures of every tile: those of the last staging when the ground, the edge objects
    /// and the marks are the same, else made again.
    fn grounds_for(
        &mut self,
        scene: &Scene,
        marks: HashMap<usize, [u8; 3]>,
        width: usize,
        rotation: u8,
    ) -> Rc<Grounds> {
        let edges: Vec<[bool; 4]> = (0..scene.tiles.len())
            .map(|t| [0, 2, 4, 6].map(|d| scene.edge_object(t, d).is_some()))
            .collect();
        match self.grounds.take() {
            Some(g)
                if (g.width, g.rotation) == (width, rotation)
                    && g.tiles == scene.tiles
                    && g.edges == edges
                    && g.marks == marks =>
            {
                g
            }
            _ => {
                let placed = (0..scene.tiles.len())
                    .map(|t| {
                        let (mut out, mut counts) = (Vec::new(), Summary::default());
                        self.terrain(scene, &marks, t, &mut counts, &mut out);
                        let out = out.into_iter().filter_map(|p| p.bounds().map(|b| (p, b)));
                        (out.collect(), counts)
                    })
                    .collect();
                Rc::new(Grounds {
                    width,
                    rotation,
                    tiles: scene.tiles.clone(),
                    edges,
                    marks,
                    placed,
                })
            }
        }
    }

    /// The pictures of every item of the paint order: the tiles' own from `ground`, the others
    /// placed here with the selection marker and the faces of the units. Gives the paint order,
    /// the pictures that are not tiles' and the entity of each.
    fn place_items(
        &mut self,
        scene: &Scene,
        objects: &Objects,
        grounds: &Grounds,
        looks: &[tank::Look],
        extra: &Extra,
        summary: &mut Summary,
    ) -> (Vec<Entry>, Vec<Boxed>, Vec<Option<pick::Hit>>) {
        let Objects {
            metas,
            owners,
            transient,
            ..
        } = objects;
        let order = paint_order(scene);
        let mut items = Vec::with_capacity(order.len());
        let mut own = Vec::new();
        let mut hits: Vec<Option<pick::Hit>> = Vec::new();
        // The pictures of the item at hand.
        let mut fresh: Vec<Placed> = Vec::new();
        for item in order {
            // The pictures of a path, an edge object or any other object are the entity's own.
            let owner = match item {
                Item::Path(o) | Item::Edge(o) | Item::Object(o) => owners.get(o).copied().flatten(),
                _ => None,
            };
            match item {
                Item::Terrain(t) => {
                    let counts = &grounds.placed[t].1;
                    summary.tiles_cached += counts.tiles_cached;
                    summary.tiles_rendered += counts.tiles_rendered;
                    summary.ground_missing += counts.ground_missing;
                    summary.cliffs_drawn += counts.cliffs_drawn;
                    summary.cliffs_missing += counts.cliffs_missing;
                    items.push(Entry::Tile(t));
                    continue;
                }
                Item::Object(o) if o >= *transient => {
                    self.object(scene, o, &metas[o], &mut Summary::default(), &mut fresh)
                }
                Item::Object(o) if metas[o].wall > 0 && owners[o].is_none() => {
                    self.pending_wall(scene, metas, o, summary, &mut fresh)
                }
                Item::Path(o) | Item::Object(o) => {
                    self.object(scene, o, &metas[o], summary, &mut fresh)
                }
                Item::Edge(o) if scene.wall_tank(o).is_none() => {
                    self.object(scene, o, &metas[o], summary, &mut fresh)
                }
                item => self.tank_item(scene, metas, looks, item, summary, &mut fresh),
            }
            // The selection marker and the faces are nobody's.
            let owned = fresh.len();
            if let (Item::Object(o) | Item::Edge(o), Some(hit)) = (item, owner)
                && let Some(&Placed { x, y, .. }) = fresh.last()
            {
                if Some(hit.id) == extra.selected {
                    self.marker(metas[o].ty, extra.form, &mut fresh);
                }
                if scene.objects[o].kind == Kind::Unit {
                    for &(_, likes, age) in extra.faces.iter().filter(|f| f.0 == hit.id) {
                        self.face(metas[o].ty, (x, y), likes, age, &mut fresh);
                    }
                }
            }
            for (k, p) in fresh.drain(..).enumerate() {
                if let Some(b) = p.bounds() {
                    items.push(Entry::Own(own.len()));
                    own.push((p, b));
                    hits.push(owner.filter(|_| k < owned));
                }
            }
        }
        (items, own, hits)
    }
}

/// The tanks of the scene, with the fence kind of each object (a habitat fence makes a wall).
fn set_tanks(scene: &mut Scene, zoo: &Zoo, metas: &[Meta]) {
    let cut = |n: u32| i32::try_from(n.min(order::MAX_ELEVATION as u32)).unwrap_or(0);
    scene.set_tanks(
        &zoo.tanks
            .iter()
            .map(|t| TankSpec {
                tile: t.tile,
                gate: t.gate,
                height: cut(t.height),
                level: cut(t.level),
            })
            .collect::<Vec<_>>(),
        &metas
            .iter()
            .map(|m| habitat_fence(m.ty))
            .collect::<Vec<_>>(),
    );
}
