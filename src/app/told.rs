//! What a pass tells outside the log lines: the sounds placed in the view, the status bar and the
//! animals' messages.

use super::*;

impl App<'_> {
    /// Takes the sounds the units' animations began and places each one: at the unit's position
    /// in the view space of the camera, heard from the centre of the map view (sound 5).
    pub(super) fn unit_sounds_placed(&mut self) -> Vec<UnitSound> {
        let Some(game) = &mut self.game else {
            return Vec::new();
        };
        let sounds = game.world.units.take_sounds();
        game.placed(
            sounds
                .into_iter()
                .map(|(id, heard)| (id, heard.file, heard.voice, heard.base)),
        )
    }

    /// Takes the ambient one-shots the steps asked for (sound 8.1) and places each one at its
    /// offset from the centre of the map view; each entry of a group is its own voice.
    pub(super) fn ambient_placed(&mut self) -> Vec<UnitSound> {
        let shots = self.sim.take_shots();
        let Some(game) = &self.game else {
            return Vec::new();
        };
        let listener = game.listener();
        (shots.into_iter())
            .map(|s| {
                let at = (listener.0 + i64::from(s.dx), listener.1 + i64::from(s.dy));
                let (distance, pan) = positional(at, listener);
                UnitSound {
                    id: 0,
                    voice: s.voice,
                    file: s.file,
                    base: 0,
                    at,
                    distance,
                    pan,
                }
            })
            .collect()
    }

    /// The loops the tank filters play now, each placed in the view like a unit's sound; the voice
    /// of a loop is its filter's.
    pub(super) fn loop_sounds_placed(&self) -> Vec<UnitSound> {
        let Some(game) = &self.game else {
            return Vec::new();
        };
        let loops = game.world.filter_loops(self.renderer.catalogue());
        let loops = loops
            .into_iter()
            .map(|l| (l.id, l.file, format!("filter {}", l.id), l.base));
        // A vehicle's loop is tried once, at its first update (the shell keeps to the `vehicle`
        // voices: ambients 6.2).
        let vehicles = (game.vehicles.iter())
            .map(|&(id, file, base)| (id, file.to_string(), format!("vehicle {id}"), base));
        game.placed(loops.chain(vehicles))
    }

    /// Writes the three bars (when they changed), and the date and the money text at each refresh, and
    /// shows the zoo status' messages in the message line; a donation names its amount.
    pub(super) fn status_told(&mut self, bars: Option<[i32; 3]>) {
        let messages = self.sim.take_messages();
        playing!(self, game, ui);
        if bars.is_some() {
            let date = self.sim.calendar().date();
            let text = to_cp1252(&format::date(&ui.string(DATE_PATTERN), &date));
            ui.set_text(STATUS_DATE, &text);
            let day = format::date(&ui.string(DAY_PATTERN), &date);
            let tip = format::expand(
                &Words::new(|_| String::new()),
                &ui.template(STATUS_DATE),
                &[Arg::Text(&day)],
            );
            ui.set_string(STATUS_DATE, tip);
            let (units, _) = game.money.bar();
            ui.set_text(
                STATUS_MONEY,
                &to_cp1252(&Symbol::new(ui.string(SYMBOL)).text(units)),
            );
        }
        if let Some(bars) = bars.filter(|b| game.bars != Some(*b)) {
            game.bars = Some(bars);
            for (id, value) in [STATUS_ANIMALS, STATUS_GUESTS, STATUS_ZOO]
                .into_iter()
                .zip(bars)
            {
                ui.set_status(id, value);
            }
            self.money_events
                .push(format!("bars {} {} {}", bars[0], bars[1], bars[2]));
        }
        for (id, amount) in messages {
            let text = ui.string(id as i32);
            let text = match amount {
                Some(units) => {
                    text.replacen("%s", &Symbol::new(ui.string(SYMBOL)).text(units.into()), 1)
                }
                None => text,
            };
            ui.message(&to_cp1252(&text));
        }
    }

    /// Takes the animals' log lines of the last pass and shows their messages, with the animal's
    /// name in them, in the message line.
    pub(super) fn animal_told(&mut self) {
        let Some(game) = &mut self.game else {
            self.lines.animal.clear();
            self.lines.guest.clear();
            self.lines.staff.clear();
            self.lines.building.clear();
            return;
        };
        self.lines.animal = game.world.units.take_animal_lines();
        self.ui_sounds.extend(game.world.units.take_cheers());
        self.lines.guest = game.world.units.take_guest_lines();
        self.lines.staff = game.world.units.take_staff_lines();
        self.lines.world.extend(game.world.units.take_world_lines());
        self.lines.building = game.world.units.take_building_lines();
        for (message, id, names) in game.world.units.take_messages() {
            let own = (game.world.find(id))
                .and_then(|at| game.world.zoo.entities[at].common.as_ref())
                .map_or("", |c| &c.name);
            if let Some(ui) = &mut self.ui {
                let names = if names.is_empty() {
                    vec![own.into()]
                } else {
                    names
                };
                let text = (names.iter()).fold(ui.string(message), |t, n| t.replacen("%s", n, 1));
                ui.message(&to_cp1252(&text));
                research::alert(ui, self.renderer.vfs(), message, &text);
            }
        }
    }

    /// Tells what the cash did since it was last told: the amount in whole units for the log,
    /// and the status bar, which shows it with its colour at every change.
    pub(super) fn tell_money(&mut self, now: u64) {
        if let Some(game) = &mut self.game {
            let (units, tone) = game.money.bar();
            if game.told != Some(units) {
                game.told = Some(units);
                self.money_events.push(format!("cash {units}"));
            }
            if let Some(ui) = &mut self.ui {
                let text = Symbol::new(ui.string(SYMBOL)).text(units);
                let shown = Some((text, tone));
                if game.bar != shown {
                    let (text, _) = shown.as_ref().expect("just made");
                    ui.set_text(STATUS_MONEY, &to_cp1252(text));
                    ui.set_fore_color(STATUS_MONEY, tone.rgb());
                    self.money_events
                        .push(format!("bar {text} {}", tone.name()));
                    game.bar = shown;
                }
                let updated = self.fired.interface.is_some();
                let lines = game.finances.refresh(&game.money, ui, now, updated);
                self.money_events.extend(lines);
            }
        }
        self.lines.money = std::mem::take(&mut self.money_events);
    }
}
