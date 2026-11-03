//! The text of `rustyzoo` without a command, and of a command that is not understood.

pub const USAGE: &str = "\
usage: rustyzoo [--data <dir>] <command>

  window-smoke        open a window and the default audio device, present one frame, exit
  ls [pattern]        list the game's files, sorted; in the pattern * matches any text
  cat <member>        write one of the game's files to standard output
  which <member>      show which file or archive supplies it, and the copies it hides
  dump-strings <dll> [--text <name>]
                      print the string table of a resource DLL of the install folder (for
                      example lang0.dll) as ID<TAB>text lines sorted by ID, with \\n, \\t, \\r,
                      \\\\ and \\u{XXXX} (other control characters) escaped, then the name and
                      size of its TEXT entries; with --text, write the bytes of one TEXT
                      entry to standard output instead
  strings             print the texts of all lang*.dll of the install folder as the game
                      resolves them: the libraries in lookup order (LIB lines), then
                      ID<TAB>library<TAB>text and TEXT<TAB>name<TAB>library<TAB>bytes
  types               list the entity types: class, type, subtype, name id, footprint (half
                      tiles), the .ani member of the base animation and whether it exists;
                      then counts per class; warnings go to standard error; exit 1 only if the system
                      fails to read a file
  check-behaviours [--type NAME]
                      read the behaviour sets and ambient tables of every .ai, .uca, .ucb and
                      .ucs file, every copy of it (also those another archive hides), and
                      count the calls per operation name (name, tab, count; the most used
                      first), then one line: files with sets or tables, sets, calls, ambient
                      tables, the calls of a name the program does not know but the data
                      holds (they fail when run) and the calls whose arguments make them fail
                      (listed as warnings on standard error). A call of any other name is an
                      error that names the file, the section and the line: exit 1. With
                      --type, print the ambient rows and the calls, as parsed, of the first
                      entity type with that type name, from the loaded catalogue, instead
  check-config        read every configuration file of the game, report empty files and ignored text
  dump-sprite <member> <dir>
                      write the frames of a sprite file as PNG files into <dir>, and print them
  check-sprites       decode every sprite file; list those that fail or have a palette
                      defect (a warning); exit 1 if one fails
  movie-info <member|path> [--dump K <dir>]
                      read an intro movie (an AVI file with Indeo 5 video and PCM audio): the
                      member of the game, or a file of this name outside the game, which wins;
                      print its picture size, frame count, frame rate, duration and audio
                      format, then decode every frame and print how many were decoded; exit 1
                      if the file cannot be read or a frame cannot be decoded. With --dump,
                      write every Kth decoded frame (K at least 1, frame 0 first) to
                      <dir>/frame-<number>.png
  map-info <member|path>
                      print the version, size, tiles and entities of a map or saved game (a .zoo):
                      the file of this path if it exists, else the member of the game
  check-maps          read every .zoo file; list those that fail; exit 1 if one is corrupted
  check-exhibits <map>
                      find the exhibits of the maps matching <map> (a member, or a pattern with *
                      as for ls) from their fences, compare their number with the habitat count
                      the file stores and check that each stored first tile lies in a different
                      exhibit; the same for the tanks (an exhibit closed by tank walls only); list
                      the sizes; exit 1 if one differs or none matches
  check-rating <map>  rate the exhibits of the maps matching <map> for each species living in
                      them (habitat rating): the shown suitability, the part scores, the
                      recommendation bits (important ones marked !) and the attractiveness;
                      also the viewing areas of each exhibit (tiles, size, tiles seen) and its
                      fence deterioration; exit 1 if none matches, a rating is not 0 to 100 or
                      an area covers more than 4 columns or rows
  find-path <map> <x,y> <x,y> --unit <keeper|guest|animal> [--task <name>]
                      the path the finder gives a unit of that kind from the first tile to the
                      second of a map (a .zoo member), as the game asks for it (a partial path
                      accepted): a line with the state (found, partial or no path), the number
                      of tiles and the move cost, then one x,y line per tile; the [AI] settings
                      are those of the ini file; --task names the unit's task by the original's
                      word (leave, avoid, chase, gotoshow, ...), which changes the costs it pays
  render-ani <member.ani> <dir> [--anim NAME]
                      write frame 0 of an animation set as eight PNG files into <dir>, one per
                      screen direction, and print the anchor point; with --anim, the first
                      argument is an animation root (such as animals/lion/f) and NAME the
                      animation under it
  check-ground        render every ground picture of the terrain cache (ztats/, ztatb/) again and
                      compare it with the cached one; exit 1 if any covered pixel differs
  render-ground <key> <out.png>
                      render one ground picture to a PNG file; the key is written as in the
                      cache, for example ztats/000003 or ztatb/00/00000000/00000001/0003
  render-map <map> <out.png> [--rotation N] [--zoom Z] [--view X Y W H] [--no-units] [--grid]
                      draw a whole map (a .zoo member) into a PNG, seen from rotation N (0 to
                      3, default 0) at zoom Z (-2 zoomed out, 0 normal, 2 zoomed in), and
                      print what was drawn: the tiles whose ground picture is in the game's
                      cache, the ground pictures missing, the cliff pieces, and the objects
                      drawn or lacking a type or an animation; with --view, draw only the
                      window of W by H pixels whose top left pixel is the view-space point
                      (X, Y) of that zoom, black where the map has nothing (the summary still
                      counts the whole map); with --no-units, leave out animals, guests, staff
                      and ambient animals, which then are not counted either; with --grid,
                      draw the grid lines of every tile
  check-render [--rotation N]
                      draw every .zoo file in memory and print the same summary per map and
                      for all of them; a file that cannot be read is skipped; exit 1 only if
                      a map cannot be drawn
  sim <map> --ticks N [--dt MS] [--seed S] [--script FILE] [--reset-fences] [--hash|--log]
                      run a map (a .zoo member) for N steps of MS milliseconds (default 67)
                      without an interface, with the generator seeded from S (default 1), and
                      print the hash of the state, the same on every system (--hash, the
                      default), or the log lines of each step (--log; the units run their behaviour sets as in
                      `run`). The events a loaded zoo posts (cans, fence pieces, filters) are posted. The fence pieces stay as the map stores them; --reset-fences makes
                      them all healthy first, as a start whose map file has `resetfences` above 0
                      does (a map of a version older than 55 has its pieces made healthy anyway). The script has one
                      `<step> <command>` per line, applied before that step (the first is 0):
                      remove ID, bulldoze ID, tile TX TY ELEVATION SHAPE TERRAIN, fill ID, drain ID, cash N, leave ID,
                      animal TYPE X,Y [male|female], mood ID N (happiness, -100 to 100),
                      save FILE (write the zoo), load FILE (open a saved game as Continue does)
  check-bitmaps       convert the root bitmaps of the terrain (grid lines, highlight, cliff
                      pieces) as the engine does when their copies in ztst/ are missing, and
                      compare each with its shipped copy; exit 1 if one differs or fails
  check-wavs          decode every .wav file; count them by format; exit 1 if one fails
  check-music         open the default audio device (SDL_AUDIO_DRIVER=dummy for none), queue the
                      menu music paused and print its format, gain and queued bytes
  check-anis          load every animation set and the sprite files it names; list the sets
                      as ok, warning (defective palette) or FAILED, then the counts; exit 1
                      if a set fails
  run [<map>] --headless (--passes N --clock MS [--script FILE] [--size WxH] | --replay LOG)
      [--png-every K] [--out DIR] [--show ID]... [--rows ID N]... [--text ID TEXT]...
      [--ask ID BUTTONS] [--bare] [--cash N] [--seed N]
                      play a map (a .zoo member) without a window. N passes of the main loop
                      with a scripted clock that advances MS milliseconds per pass, the
                      commands of the script applied before their pass (one `<pass> <command>`
                      per line: scroll DX DY, pointer X Y, pointer none, rotate cw|ccw, zoom
                      in|out, grid (toggle the tile grid), devmode (Ctrl+D), fullscreen on|off, pause, space, quit, close (the window was closed), and for the
                      interface mouse X Y, down, up, double (the second press of a double click), right (the right button went up),
                      wheel N, key NAME [ctrl] [shift] with NAME enter, escape, tab,
                      backspace, delete, left, right, up, down, home, end, pageup, pagedown, y
                      or n, and char C with C a byte or one character, save FILE and load FILE (write the game, open a saved game; the name runs to the end of the line); # starts a comment).
                      The interface is drawn over the map, with the layouts given by --show
                      (each ID shown, repeatable) and the map view; the keys and the edge
                      scroll of the map act only while the map view is shown and no field has
                      the focus. The mouse commands reach the widgets (hover, buttons of every
                      kind, radio sets, the actions of buttons, draggable layouts, scroll
                      bars, list boxes, text fields) and a line `ui ...` tells what they did
                      (hover, sound, press, release, select, deselect, show, hide, drag, scroll
                      ID POSITION, row ID ROW, select ID ROW for a list, focus, unfocus, text
                      ID TEXT, beep ID for a refused character, key LAYOUT KEY, tooltip X Y W H when a tooltip shows, ask and
                      answer CODE for a question); the keys and characters go to the focused
                      field or list, and then to an open question (Enter, Escape, y, n), which
                      alone receives the mouse and pauses the game; a message over the map
                      view belongs to the map: a press or release on it prints `tool down X,Y` or `tool up X,Y`, the tile
                      under the pointer (`none` off the map), and the first draw after the
                      world was edited prints `world stage`; the construction, objects, Adopt and Hire panels of
                      the left bar fill their item lists and log `tool panel construction|objects|adopt|hire|none`,
                      `tool item TAB CLASS/TYPE/SUBTYPE name=NAME cost=COST` for the item chosen, `tool
                      rotate PANEL STEP` and `tool drop` (Escape or the right button drops the item,
                      then closes the panel); the item follows the pointer over the map, snapped
                      and tinted by its place (green valid, orange replacing a piece, red not
                      valid) with its cost 23 pixels right of the pointer, and each move logs
                      `tool pending CLASS/TYPE/SUBTYPE X,Y q N valid|overwrite|highlight|invalid` (the tile and
                      the quarter tile of the snapped point); releasing over a valid place of
                      scenery or a building places it when the cash covers its cost (a path is
                      dragged: the pieces of an L from the press tile, the first leg along the axis
                      of the first tile entered, bought in order, `world paint X,Y 3` first on a
                      water tile, `world retype ...` for a path or a fence of the same type it replaces, which keeps
                      its id, and `world remove ...` then `world add ...` for a fence of another type; a fence is dragged
                      along the side of its tile the press is nearest to, `smartFences` of `[UI]`
                      keeps it off large objects; `world add CLASS/TYPE/SUBTYPE X,Y id N quadrants Q`
                      (a recolourable building stores the default colour of each group), `X,Y
                      side S id N` for a fence, `id N name=NAME` for an animal of the Adopt panel (the bulldozer sells one) or a staff member of the Hire panel (the bulldozer fires one, after a question that names the salary, and refunds nothing; `money book CATEGORY PRICE` tells where the price went), `tool turn STEP` for a type that turns by
                      itself, the placement sound), one that is not valid or too dear asks for the
                      refusal sound and shows its message, if it has one, in the message line for
                      5 seconds (`tool message ID`; `tool refused` when it has none). With no tool, a click on a unit
                      selects it (`tool select ID`; an arrow marks it on the map) and a click on the selected one picks it up (`world carry ID`):
                      it follows the pointer at $0 with the checks of a new unit, a release on a valid place
                      puts it down (`world drop ID X,Y`, `home N` for an animal in an exhibit), Escape, the
                      right button or choosing an item puts it back (`world putback ID X,Y`). The bulldozer button (1028) of the main panel
                      and the Delete key make the bulldozer the tool (`tool bulldozer on`, off
                      again by the same, Escape or the right button): the object under the pointer
                      is tinted red with its refund next to the pointer (`tool bulldoze CLASS/TYPE/
                      SUBTYPE id N refund R name=NAME`, `tool bulldoze none`), a press and a drag
                      outline the rectangle of tiles and tint every object in it (`tool bulldoze
                      area X,Y X,Y objects N refund R`), and the release removes them, 80% of the
                      cost back (a fence 40% when decayed, nothing when broken), booked as a
                      construction refund (`world remove CLASS/TYPE/SUBTYPE X,Y [side S] id N
                      refund R`, `tool bulldoze removed N refund R` after a drag, `exhibit gate ID X,Y
                      side S` for the piece that became an exhibit's gate); a building that needs it asks
                      first unless `[UI] confirmSell` is 0, a fence piece whose line opens or
                      merges exhibits asks (`tool bulldoze ask Sell|Merge(X, Y)|Escape(X)`, answered
                      by `key y` or `key n`, `tool bulldoze answer yes|no|cancel`). A tank wall whose
                      tile across is not in its tank and holds a tank wall on the same line has it
                      tied: both are tinted and priced, and one click removes both, so the line
                      between two tanks goes in one click and the tanks merge.
                      A fence that closes an area makes an
                      exhibit (a fence that cuts one makes a second one): `exhibit new ID \"NAME\"
                      tiles N gate none` and `sound exhibit ID` (the window plays `sounds/exhibit.wav`
                      once) open the name dialog (layout 45300), which pauses the game
                      and holds the rest of the drag, the new exhibit having no gate yet and the gate
                      button staying disabled; `exhibit name ID \"TEXT\"` follows the answer when
                      the text has a letter, digit or punctuation mark (else the default name
                      stays), then `exhibit gate ID X,Y side S` (`tool message 11000` or `11001`
                      when no keeper can reach it) and the rest of the drag. `exhibit split ID tiles
                      N` is the part that keeps its name and gate, `exhibit removed ID` and `exhibit
                      merged ID into ID tiles N` the others, and `exhibit gate ID X,Y side S` also
                      tells a new gate after the gate piece went; the gate button of the fence tab
                      is enabled while the zoo has an exhibit that is named. Pressing it makes
                      the gate the tool (`tool gate on`, off again by the same press, an item, another
                      tab, Escape, the right button, closing the panel or the last exhibit going):
                      over a fence piece of an exhibit the pending object is the gate of its type,
                      valid when one side of the piece's line is an exhibit and the other the
                      grounds, the slot across is empty, the piece is not the exhibit's gate, no
                      object stands on the tile of a tank (red, and no cost text) and the cash
                      covers it, else red (the price shown); a valid gate is green where a keeper passes
                      (the crossing test, with the marine specialist in a tank, gives code 0 and a cost below 10) and yellow (the
                      highlight colour, logged as `highlight`) elsewhere, a click still placing
                      it; a gate costs nothing unless the piece is at or below its decayed life,
                      and the cost text shows its price (not as a charge). Over a tank wall it has the wall height
                      of the tank, drawn as that stack with its ladder and its own platform. The gate is judged
                      when the piece under the pointer changes (after a click none is shown until
                      the pointer has been over another piece), and no tile is outlined. A saved
                      exhibit's gate is the piece its record names. A click on a valid piece turns it into the gate, healthy, and the exhibit's
                      old gate back into a fence (`world ungate CLASS/TYPE/f X,Y side S id N`,
                      `world gate CLASS/TYPE/g X,Y side S id N`, `exhibit gate ID X,Y side S`),
                      books the price as construction and clears the undo list; any other place
                      does nothing and says nothing (a gate the cash did not cover when the pointer
                      reached the piece shows its price, red, and a click does nothing), except that
                      a gate the cash no longer covers at the click shows `tool message 10002`. The undo button (1075, grey
                      until something can be undone) and the Backspace key take back the actions
                      of the last press and release, the last first (`tool undo N`): a placed
                      object goes with its full price back (`world remove CLASS/TYPE/SUBTYPE X,Y
                      [side S] id N undo`), a removed one comes back with its rotation, life and
                      state and its refund is taken back (`world add ... undo`), a piece laid on
                      another gets its old type back (`world retype ... undo`), terrain tiles are
                      restored and the price of an accepted session given back (`terrain undo
                      N`), and an exhibit that a fence cut or opened gets its name and gate back
                      (`exhibit ...` lines); no name is asked. A removed object that cannot be
                      put back (`world blocked ...`) stops the undo with message 159 and keeps
                      the rest of the list, except among several removed objects, when it is
                      passed over (`world skip ... undo`). The first action of a press empties the
                      list, a press that does nothing keeps it. A fence that closes
                      an area of tank walls only makes a tank (\"Tank N\" or \"Show Tank N\"): its
                      ground is levelled flat, lowered by `initialSink` of tanks.cfg and given the
                      floor `tankTerrain` (`tankTerrainShow` for a show tank), its walls
                      brought to its side of their lines (a new piece where another tank stands),
                      `exhibit tank ID height H level L show S salt T purity P filled F` tells its
                      values, again at each step of its water (the level rises one elevation step per simulation
                      step up to the height plus `waterOffset` while the tank is filled, only
                      while the game is not paused and no dialog is open, so after the name
                      dialog; a tank made while paused is full at once); when something stands in it that a tank does not allow the game asks
                      first (`tool tank ask`, string 10552; `key y` sells the scenery and puts the
                      animals and staff outside, `tool tank answer yes`, then the name dialog; any
                      other answer, `tool tank answer no`, undoes the drag and gives its money
                      back). A land exhibit closed by tank walls only becomes a tank this way, and
                      a tank that gets a wall of another kind a land exhibit, each a new exhibit
                      with a new name. The two
                      terrain tabs of the construction panel (at 180,195 for the paint tab and 180,228 for the modify tab of a 1024x768 window)
                      open the terrain panel: the brush (a square of 1 to 5 tiles, the plus and
                      minus buttons or the `+` and `-` keys) follows the pointer over the map, and
                      painting a type of the list with a press and a drag gives it to the tiles of
                      the brush that accept it (the paint tab writes minus the cost of painting
                      the brush, 0 after a paint, 23 pixels right of the pointer, and a tile the
                      pointer reaches shows the help of its terrain type as a tooltip); in the modify tab a drag up or down raises or lowers
                      the brush one step per 16 pixels (8 and 32 at the other zooms), by the mode
                      (contiguous or not, drag or level to the tile first clicked; a step that does not keep the ground
                      joined takes the other tiles of an object larger than a tile that stands on the brush along); the session costs what the tiles touched cost
                      against their first state, shown in the panel; accept pays it (booked as
                      construction), cancel puts the tiles back, and leaving the panel (another
                      tab, closing it, Escape) accepts it when the cash covers it and else cancels
                      it with a message. The tools switch the tile grid on while they act. The log
                      has `terrain enter paint|modify`, `terrain brush SIZE X,Y` (the top left tile),
                      `terrain size N`, `terrain type N`, `terrain paint N` (tiles painted), `terrain amount N|none` (the amount next to the pointer changed),
                      `terrain shape N` (tiles reshaped), `terrain mode ID`, `terrain cost N`, `terrain accept N`, `terrain cancel N`, `terrain leave` and
                      `terrain short`. The cash is told as `money cash N`
                      (whole units) when it changes, with `money bar TEXT COLOUR` for the
                      status bar, `money set N` and `money add N` for a start, and `money
                      month M year Y` at a month change; the `$` character adds 10000 to the
                      cash as a donation; the Z character presses the zoo status button (1014),
                      which opens or closes its window; `money cell ID RRGGBB TEXT` tells each
                      cell of its Finances table that changed, when the tab is shown and at
                      most every 10 s of clock while the window is shown (an older month's
                      column is blank before the accounts begin). A release on the map with no tool picks the
                      topmost entity under the pointer, and the gate of an exhibit opens the exhibit panel on it and is the selected entity (an arrow marks it)
                      (the exhibit list button, 1050, shows the panel with no exhibit; a click on a row of
                      its list makes that exhibit the current one and centres the view on its gate; the name
                      field renames it; its tabs show the construction date, upkeep and keeper, the popularity
                      and donations, the animals and the twenty newest thoughts about the exhibit); a click on an animal (or on a row of the
                      Animals tab, which also centres the view on it) opens the animal panel: the name field,
                      the four bars of happiness, hunger, health and the suitability of its exhibit (green,
                      yellow or red as its habitat check judges it), the sex and when it last ate and slept,
                      and the five newest thoughts about it, refreshed every 5 s of clock while shown and not while the game is paused; its
                      recommendations button shows the plaque of that animal, its exhibit button the
                      exhibit panel on its exhibit (nothing on the grounds), its follow button (showing its type's idle animation, or its egg's or crate's, seen from the south-east, centred and clipped, playing from its first frame while the game is not paused, set when the panel opens and at a forced refresh such as a tab click) keeps the view on it, and a click
                      anywhere else on the map closes it (the sell and pick-up buttons do nothing yet). `panel animal ID
                      \"NAME\" happiness H hunger N health L suitability S fed WORDS slept WORDS` tells what it
                      shows (` thoughts [IDS]` closes the line when guests have thoughts about it: their string ids,
                      newest first), when it opens and when a value changes, and `panel animal none` when it closes. A click on a
                      staff member (a second click picks it up) opens the staff panel: the name field, the monthly
                      salary, the text of its job, and a second tab with the exhibits assigned to a keeper or tour
                      guide or the duty switches of a maintenance worker, refreshed every second of clock while
                      shown and not while the game is paused; its fire button asks as the bulldozer does, its
                      pick-up button picks it up, its follow button shows its type's picture (set likewise), and its assign button (the second tab) starts the assign mode:
                      the exhibit under the pointer is tinted green when the keeper may serve it and red when
                      not, a click assigns it (or shows why not) and appends it to the list, a row of the list
                      centres the view on that exhibit's gate and opens the exhibit panel, the clear buttons
                      remove one or all, and Escape or the right button end the mode. `panel staff ID \"NAME\"
                      salary S duty \"TEXT\" exhibits [NAMES]` (`duties 1111` for a worker) and `panel staff none`
                      tell what it shows; `tool assign over N good|bad|none`, `tool assign ID N` and
                      `tool unassign ID row R|all` tell the assign mode. A click on a guest (a second click picks it up) opens the guest panel: the name field, the five bars
                      of happiness, drink, food, restroom and energy (the needs as 100 minus their percentage, so a full
                      bar is a satisfied guest), and on the other tabs the time in the park, the favourite species, the
                      items held and the five newest thoughts of the guest, refreshed every 10 s of clock while shown and
                      not while the game is paused; its pick-up button picks it up and its follow button (showing its type's picture, set likewise) keeps the view
                      on it. `panel guest ID \"NAME\" happiness H drink D food F restroom R energy E time \"TEXT\"
                      favourite \"NAME\" thoughts [TEXTS]` tells what it shows, when it opens and when a value changes, and
                      `panel guest none` when it closes. The research button (or the R key) shows the research panel of a game with a research tree
                      (a scenario or freeform start, a saved game that names one): the Status tab, and the two branch tabs with the category ticks, the funding buttons and the
                      current program, refreshed every second of clock while shown and not while the game is paused; `panel research tab N ...` tells what it shows when it changes.
                      A completed program opens an OK dialog the game runs on under, and a second one when its branch has nothing left to research. A click on a guest building (a burger stand, a restroom) opens its
                      panel: a building that charges has the price field with up and down buttons (0.25 a press, within the
                      economy's building price maximum and the type's low and high cost; the field of the full game can be
                      typed into, the number going through the same bounds), the income, upkeep and profit since built and per month, the months open
                      and the items sold (refreshed every 5 s of the clock); a type that hides the cost change (a grandstand) hides the buttons and disables the field, one that hides the commerce info leaves everything below the price unfilled and hides two tab buttons; both panels show the type's information image and the name field renames the building (a text with no letter, digit or punctuation mark keeps the name); any other building shows its guests and months open
                      (and on the full game its monthly upkeep), filled when it opens. Its Programs tab (hidden while the type has no building upgrade, research 6.3) shows up to five upgrade buttons, the current one pressed, with the current upgrade's upkeep and its adult and child bonuses (a bonus of -1 is hidden); a click takes that upgrade for the building (`building upgrade ID N`, the building's upkeep and the happiness its leaving guests gain follow it) and the panel line then ends with ` upgrades N current I upkeep U adult A child C`. `panel building ID \"NAME\" price P income I
                      upkeep U profit R month I U R months M guests last A this B total C sells [ITEMS]` tells what it shows, when
                      it opens and when a value changes (` fixed` follows the price when the buttons are hidden, and a panel with nothing filled below the price tells `price P`), `building price ID P` each change of the price, and
                      `panel building none` when it closes. A click on a tank filter (Marine Mania) opens its own panel (layout 46): the state (broken, minimum or maximum efficiency; no text for a filter switched off), the months since its last service and in operation on the status page, and the upkeep of the last 30-day period, the current one and since it was built on the finance page (periods counted from the build date, checked every 3 s of play; refreshed when it opens and every second of the clock); its sell button refunds the filter's own price, and `panel filter ID \"NAME\" STATE service N months M upkeep A B C` and `panel filter none` tell what it shows. The sell button of the panel sells the building as the bulldozer does (the
                      question with the refund when `confirmSell` asks for one) and the pick-up button, shown for a type with `cMoveable` 1
                      only, picks it up as a second click on the selected building does (`world carry ID`): it follows the pointer free of
                      charge with its facing, a release on a valid place puts it down (`world drop ID X,Y`) and Escape, the right button or
                      choosing an item puts it back (`world putback ID X,Y`); a drop makes the animals of the exhibit react as a new building does; an undo of its placement while it is held removes it with its whole price; the sell button clears the selection first, so a No leaves the panel closed; both push its users out first (`building push ID GUEST`). `panel exhibit ID \"NAME\"`, `panel status DATE upkeep A B C staff Yes|No`, `panel
                      donations stars N A B C`, `panel animals N thoughts N` (` [IDS]` follows a number above 0) (or `panel exhibit none`) tell
                      what the panel shows, `panel plaque \"ANIMAL\"` and one `panel line TEXT` per
                      recommendation (`panel line!` for an important one, written in red) what the plaque shows,
                      each time something changed: when it opens, at once when its list is clicked or a name is
                      set, and every 10 s of clock while it is shown, not while the game is paused. --cash N is the cash a map starts with instead of
                      the one it stores (any finite number). --seed N (0 to 4294967295) makes a test run: the game's
                      random generator is seeded with N at the start and N+1 when the first map is set up (at once with
                      a map, else when the menu starts a game), and with N+2, N+3, ... each time a map of
                      another size is set up or a month ends, instead of the wall clock; the lines `sim seed S`
                      (each seeding, also from the wall clock), `sim step MS` (each step the game runs) and `sim month M Y` (the step that acts on the
                      month flag of the one before, followed by `sim hash H`, the state hash that `sim --hash`
                      prints, after that pass) tell it, so two runs with one seed print the same lines (a
                      replay of a log seeds the generator with the seconds the log tells). The zoo status
                      logs `sim rating N` at each rating check (after the set-up check of a new game, the first
                      periodic one comes 4 s later), `sim message ID` for each advice message and `sim donation
                      AMOUNT`, and the world manager `sim rate ID KEY SHOWN ...` each time it rates exhibit ID
                      (the suitability each species shows); the three bars and the date (month and year) of the status bar are written once a second and `money bars A G Z`
                      (animal happiness, guest happiness, rating) tells a change. The units of a
                      map (animals, guests and staff) log `unit add ID KIND X,Y` when a game opens or one is
                      placed, `unit remove ID` and `unit status ID` at each unit's status tick, once a second of
                      simulated time, and `unit tile ID X,Y`, `unit enter ID EXHIBIT`, `unit face ID DIRECTION`,
                      `unit abort ID` and `unit gate ID open|close GATE` when a unit walks (it enters a tile or another
                      exhibit, turns, is refused its next step, or opens the gate ahead of it or closes the one behind it);
                      a unit entering a tank tile adds `unit depth ID Z`, its height in movement units (16 to a level). A unit's goals log `unit goal ID TASK MODE TARGET TILE` when its task changes (the task
                      by the original's name, `-` for none) and `unit job ID JOB` when its job does (the jobs of staff are the `staff job` lines below); an idle unit wanders
                      (task `random`), and a guest stands between two walks. An animal adds `animal status ID HAPPINESS HUNGER ENERGY HEALTH` (the
                      values after the update of that tick) and `animal message STRING ID` for a message it
                      posts and `animal trample ID X,Y` for a tile it tramples; an animal in a tank adds `animal breath ID N` for the breath it gains under water (at its threshold it swims to the surface: the task `surfacebreath`) and `animal terrain ID MODE` when the water check changes its terrain mode (0 land, 1 water, 2 tank); an animal logs `animal likes ID` or `animal dislikes ID` when an object, an animal or a staff member placed, put down after a pick-up or removed, a terrain change or a new wall height of a filled tank with a gate in its exhibit raises or lowers its species' rating there (an animal or an object decides by the species' values, negated for a removal, when the rating stays the same), and shows a smiling or frowning face above itself for 1250 ms; the change then logs `animal sound smile` if an animal (not an egg) reacted with a like and `animal sound frown` if one reacted with a dislike, and a window plays `sounds/smile.wav` or `sounds/frown.wav`. The new guest check lets a guest in at the zoo entrance and logs `guest add ID adult|child` (the admission is booked); each status tick of a guest logs `guest status ID HAPPINESS HUNGER THIRST BATHROOM ENERGY`; a guest that goes to see an exhibit logs `guest view ID EXHIBIT` when it starts viewing and `guest donate ID EXHIBIT AMOUNT` when it leaves the viewing area with a donation (booked as income); a guest that decided to leave (or is told to) walks to the entrance and is removed there (`guest leave ID`). A guest logs `guest thought ID TEXT` for each thought (a string id) and `guest litter ID X,Y` when it drops a piece of litter on a tile. A keeper logs `staff go ID EXHIBIT` when he starts for the exhibit that needs him, `staff job ID JOB` at each job of his duties there (`PutFood`, `PuttingFood`, `HealAnimal`, `HealingAnimal`, `CleanDirt`, `CleaningDirt`, `none`) and `staff done ID EXHIBIT` when it is serviced; the food and the medicine he buys are booked as animal food and animal health. A maintenance worker logs `staff job ID JOB` (`TrashCan`, `EmptyTrash`, `TrashPile`, `SweepTrash`, `ZooDoo`, `BagDoo`, `GoToFence`, `FixFence`, `none`) at each job of the task it takes from the shared pool (dung on the zoo grounds and what an exhibit left when its last animal went are bagged, and the compost building, when there is one, pays the cash 50 for a pile and 100 for a dinosaur's, as a construction refund; a keeper's raking pays it too) and `world fence ID LIFE` for each fence piece it makes healthy. Each unit runs a behaviour set
                      after its move and animation: an animal
                      chooses the set of the ambient row for its happiness, a guest or
                      staff member `bZooRoutine`; `unit set ID NAME` tells a set
                      that starts, `unit anim ID NAME` the animation the unit shows when it changes (the idle
                      one between two sequences) and `unit sound ID NAME` a sound a play call begins (the file
                      its definition's [Sounds] names; a window plays it as a positional one-shot at the unit and prints
                      `pass N t=MS sound unit ID FILE at X,Y sum S pan P` with `started`, `continued`, `too far` or `failed`); the map shows the animation and frame each unit holds.
                      The move calls, the faces and the effects on other entities are done at once with no
                      effect until their issues, and a routine call is done when the unit's play state is ready (the goals run every step anyway). --rows ID N (repeatable) fills the list box ID
                      with N made-up rows (at most 100000), --text ID TEXT gives an element a
                      text and --ask ID BUTTONS opens the question layout ID (45000 is
                      confirm.lyt) with some of the buttons yes, no, ok, cancel, comma
                      separated, for a script to scroll, select, type in and answer. A game shows
                      the map view and the main panel (layout 2); --bare (with a map) leaves the
                      panel out, so that the frames are those of the map alone. The window is
                      1024x768 unless
                      --size says otherwise. Prints the size of the window, then, for each pass
                      in which something happens: a line per command applied, then one per
                      timer that fires, draw (the hash of the frame painted, or - if none, and
                      the view), interface, or step (with the date). --replay runs again the
                      passes of such an output saved in LOG, at the size and the clock it
                      records, with its commands, and prints the same lines. A frame painted
                      in a pass whose index is a multiple of K is written to
                      DIR/frame-<pass>.png (DIR is by default the current folder)
  run [<map>] [--size WxH] [--passes N] [--script FILE] [--log FILE] [--timing MS] [--shot FILE]
      [--show ID]... [--rows ID N]... [--text ID TEXT]... [--ask ID BUTTONS] [--bare]
                      play a map (without one, start up to the main menu, see below) with a
                      game size of the ini file's screenwidth and
                      screenheight (cut to the display; --size overrides it, uncut), in a
                      window enlarged by the display scale rounded to a whole number (the
                      factor, at least 1, smaller if the window would not fit the display),
                      with the menu music; the ini file's fullscreen (not 0) starts it in full
                      screen, and every switch is written back to the settings of the user folder. The loop runs on the real clock, sleeps until the next timer
                      is due, blocks while the window is minimised or hidden, and shows a frame
                      only when one was painted. Closing the window ends it (it asks first, in a game: see
                      below). --passes ends it
                      after N passes, --script applies commands as above before the pass of
                      their index (fullscreen asks the window to switch; the real mouse is then ignored and no edge scroll runs), --log writes the
                      lines a headless run prints, so that
                      `run --headless --replay` plays it again with the same frame hashes. It
                      prints the game size and the factor, the window size, the size in pixels
                      and the output size, then
                      the display scale and the pixel density (SDL_VIDEO_DRIVER=dummy shows
                      no window). --timing ends it after MS milliseconds and prints, per
                      timer (draw, interface, step), the count, minimum, median, 95th
                      percentile and maximum of the gaps between its firings, the period it
                      should keep, and a histogram of the gaps: information only.
                      --shot writes the pixels of the last frame, as the renderer has them
                      (at the size in pixels), to FILE as a PNG, and prints its size, number of
                      colours and hash; a blank picture is an error
                      Without a map, `run` starts up as the original does: first the intro movies
                      (MSLOGO.AVI, then BFLOGO.AVI when [UI] playSecondMovie is not 0, both only when
                      [UI] playMovie is not 0, volumes movievolume1 and movievolume2; one that is
                      missing or damaged is skipped with a warning) stretched over the window, with their sound,
                      the pointer hidden in full screen; a key or a left or right button press (in
                      a script: skip, key ..., down) stops the movie that plays and the next step
                      follows. A headless run prints `startup movie FILE FRAMES frames MS ms WxH`
                      and, on the passes of the same clock, `movie FILE frame N HASH` (the picture
                      in the window), `movie FILE end` or `skipped`; --png-every writes those
                      pictures too. Then the start-up picture of the expansion
                      flags (those of the xpac*.cfg files) in the middle of the window with a
                      progress bar over it, which the steps of the loading fill (and whose count
                      is written to [UI] progresscalls of the settings of the user folder), the
                      menu music, then the main menu: Continue enabled only when [user] lastfile
                      names a file (otherwise it is emptied), Zoo Items disabled, the version label,
                      Exit ending the run (closing the window too). A headless run prints the
                      lines `startup ...` after the size and, in its first pass, `menu ...` lines
                      (continue enabled|disabled, version) and `menu exit`; a log that has them
                      replays. The first-run tutorial is not started and a line says so. The credits button shows the credits, whose pages replace one another
                      every [credits] delay seconds (8) of interface time, in the order of the registry
                      files ui/credits/credit*.cfg (descending names, version above 0): a log line
                      `menu credits page N layout ID` for each, `menu credits end` after the last and
                      `menu credits closed` when Exit closed them. The load button of the main menu (or Ctrl+L) lists the `.zoo` files of the folder of
                      `lastfile`, else of the `Saved Games` folder of the user folder, on the freeform
                      screen (`menu game load list N`); Load starts the one selected and makes it
                      `lastfile` (`menu game start load FILE`). The scenario and freeform buttons show their screens with the lists
                      filled: `menu scenarios N`, one `menu scenario SECTION won|playable|locked
                      NAMEID` per entry (ascending section names; won, playable and locked by
                      `[scenario]` of the ini file), `menu scenario select SECTION STATE picture
                      NAME story shown|hidden play enabled|disabled` when an entry is selected
                      (its picture and story, or the locked ones; Play is on only for a playable
                      one), `menu freeform N` with a `menu map FILE NAMEID` per map and `menu map
                      select ...`, and `menu cash AMOUNT hard|middle|easy up on|off down
                      on|off` for the starting cash ([UI] MSStartingCash, MSMinCash, MSMaxCash,
                      MSCashIncrement; the spinners step by the increment and stop at the limits,
                      a typed amount is read when the edit ends, and each change is written to
                      MSStartingCash of the settings of the user folder). Play or a double click on
                      an entry log `menu play scenario SECTION FILE` or `menu play freeform FILE
                      cash AMOUNT` and open the map the file names (`[start] savegame`) as a
                      saved game, as does Continue (`[user] lastfile`): the file is read as a
                      plain path (absolute, or relative to the install folder), the menu layouts
                      are hidden, the map view shows, the menu music starts to fade, and the
                      first step of the simulation is the cap; scenarios and freeform maps empty
                      `lastfile` and run the goals of their `.scn` file (`sim scenario start
                      FILE GOALS`, `sim scenario trigger ...`, `sim scenario end won|lost`; the
                      trigger 2 shows popups (`sim scenario popup
                      LAYOUT modal|open`, with a hint line, `sim scenario hint LAYOUT`) and
                      disables, enables, hides, shows and presses elements by id (`sim scenario
                      element ID disable|enable|hide|show|press`); trigger 1 shows a message
                      (`sim scenario message TEXT kind K`), 3 changes the cash (`sim scenario cash
                      HOW AMOUNT`), 5 boxes an animal at the gate (`sim scenario boxed NAME id N`,
                      or `none`), 8 removes animals (`sim scenario remove BY ID count N`) and 9
                      switches the animal panel's sell button (`sim scenario sale on|off`), 6
                      gives an award (`sim scenario award ID`, none for a `truleb` other than 0)
                      and 10 completes every research program (`sim research forced N`, N
                      programs); the research and shows rules are stubs, rule 1 `ruleb` 11 counts
                      the awards held, and rules 10 and 11 read what the interface shows and what
                      was pressed; a game that opens gives the elements back). When the scenario
                      ends, its result is
                      written to `[scenario]` of the settings of the user folder if lower than
                      the stored one, and a win appends its `[completion] unlock` types to `[user]
                      unlockEntityN` (`menu scenario ended won|lost`, `menu scenario result KEY N`,
                      `menu scenario unlock ID`); the panels of the hide-all routine hide, the
                      selection clears and a dialog (layout 45200, `menu scenario dialog TEXTID`)
                      tells the result with Yes and No; No stops the game and shows the
                      scenario screen with the win or lose picture and story (the main menu for a tutorial or a freeform map); Yes
                      starts the next tutorial after a tutorial, else the game goes on; closing
                      the window meanwhile ends the run. The Scenario button (4107, the O character) fills
                      panel 803 with the scenario's name and story and one line per goal
                      (`menu scenario list N`). The lines are `menu game start LABEL
                      FILE`; a file that cannot
                      be loaded is a question box with its message (`menu message TEXT`) and the
                      menu stays. In a game, Escape (with nothing to cancel; an open panel closes
                      first), the window being closed (`close` in a script) and the Main Menu and
                      Exit Game buttons of the options panel, and Load Game (Ctrl+L), ask whether to
                      save (`menu game ask leave|exit|load`, Yes No Cancel; Yes opens the save dialog, `menu game save dialog`, a name field that starts
                      from `lastfile`; `.zoo` is added to a name that does not end exactly in it, the
                      file is written aside and renamed (`menu game saved FILE`, which becomes
                      `lastfile`), and a failed write or a name over 256 bytes is a message; after a
                      good save the question goes on, after Cancel nothing happens; Save Game and Ctrl+S
                      open the same dialog, and Ctrl+S and Ctrl+L close the open panels and clear the
                      selection first (the options panel's buttons close nothing); a save
                      sets `lastfile` also in a run of a map; none of this asks while the Save Game
                      button is disabled): No stops the game and shows the menu again (`menu
                      game stop`, Continue checked, the music from its start unless it plays at
                      full level: `menu music restart`) or, for Exit, ends the run (`menu game
                      exit`). `quit` in a script ends the run without a question. A run of a map
                      has no menu to go back to: it ends instead.
  dump-layout [--size WxH] [--expansions N]
                      load the interface tree as check-layouts does and resolve it for a window
                      of W by H pixels (1024x768 by default; each side from 1 to 16384), then
                      print one line per element in drawing order: id, type, layer, state and
                      the rectangle on the screen (x, y, width, height), tab separated
  render-ui <out.png> [--size WxH] [--expansions N] [--show ID]... [--hover ID] [--press ID]
                      paint the interface into a PNG file (1024x768 by default): a black frame,
                      the root, and what is shown: each --show shows the layout with that id and
                      its children; --hover highlights an element and --press presses one. Prints
                      the file and its size
  text-width <text> [--font FACE] [--size POINTS]
                      print the width in pixels of the text on one line in the original's
                      font (Liberation Sans, whole-pixel advances, no kerning): FACE is a
                      face string, \"Arial\" (the default) or \"Arial Bold\", and POINTS a size
                      string (8 by default); needs no game data
  draw-text <out.png> --width W [--font FACE] [--size POINTS] [--justify left|center|right]
      [--shadow] [--fore R G B] [--back R G B] <text>
                      draw the text as the original does (wrapped at spaces and line feeds
                      to W pixels, a too long word cut with ...) on a canvas W pixels wide and
                      as tall as the text, filled with the back colour (white), text in the
                      fore colour (black), into a PNG file; prints the size and the hash of
                      the pixels; needs no game data
  check-layouts [--expansions N]
                      load ui/gamescrn.lyt and every layout it names into one tree, for the
                      expansions in the mask N (bit 1 and bit 2, the `id` of an xpac*.cfg; default: those
                      of the install), and print the elements per type, the duplicate ids and the
                      pictures loaded and missing (missing ones are warnings on standard error);
                      exit 1 if the load fails at a section without a known type or without a
                      layout; the interface is then dropped
  timing              print the periods and the cap the ini file gives the main loop, then how
                      many draws, interface updates and steps a ten second run makes with a
                      pass at every millisecond

--data <dir> is the install folder of the game (it holds zoo.ini). Without it, the
ZT_DATA_DIR environment variable is used. window-smoke does not need it.
The month and weekday names of the dates and the forms of numbers and money come from the
user's regional settings; RUSTYZOO_ENGLISH_DATES=1 makes them English and US (as the tests and
CI do).";
