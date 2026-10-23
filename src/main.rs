mod audio;
mod boot;
mod findcmd;
mod input;
mod layout;
mod probe;
mod run;
mod simcmd;
mod textcmd;
mod window;

use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::anim::{self, AnimSet, Animation};
use rustyzoo::behaviour::{self, Survey};
use rustyzoo::bitmap;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::exhibit::{rating, viewing};
use rustyzoo::ground::{self, Key, Renderer};
use rustyzoo::movie::Movie;
use rustyzoo::music::Music;
use rustyzoo::pe;
use rustyzoo::render::{MapRenderer, Options, Summary};
use rustyzoo::sprite::{self, write_png};
use rustyzoo::strings::StringTables;
use rustyzoo::timing::Timers;
use rustyzoo::userdir;
use rustyzoo::vfs::{self, Vfs};
use rustyzoo::world::World;
use rustyzoo::zoo::{Zoo, ZooError};
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

const USAGE: &str = "\
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
  map-info <member>   print the version, size, tiles and entities of a map or saved game (a .zoo)
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
                      print the hash of the whole state, the same on every system (--hash, the
                      default), or the log lines of each step (--log; the units run their behaviour sets as in
                      `run`). The fence pieces stay as the map stores them; --reset-fences makes
                      them all healthy first, as a start whose map file has `resetfences` above 0
                      does (a map of a version older than 55 has its pieces made healthy anyway). The script has one
                      `<step> <command>` per line, applied before that step (the first is 0):
                      remove ID, bulldoze ID, tile TX TY ELEVATION SHAPE TERRAIN, fill ID, drain ID, cash N, leave ID,
                      animal TYPE X,Y [male|female], mood ID N (happiness, -100 to 100)
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
                      or n, and char C with C a byte or one character; # starts a comment).
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
                      topmost entity under the pointer, and the gate of an exhibit opens the exhibit panel on it
                      (the exhibit list button, 1050, shows the panel with no exhibit; a click on a row of
                      its list makes that exhibit the current one and centres the view on its gate; the name
                      field renames it; its tabs show the construction date, upkeep and keeper, the popularity
                      and donations, the animals and the twenty newest thoughts about the exhibit); a click on an animal (or on a row of the
                      Animals tab, which also centres the view on it) opens the animal panel: the name field,
                      the four bars of happiness, hunger, health and the suitability of its exhibit (green,
                      yellow or red as its habitat check judges it), the sex and when it last ate and slept,
                      and the five newest thoughts about it, refreshed every 5 s of clock while shown and not while the game is paused; its
                      recommendations button shows the plaque of that animal, its exhibit button the
                      exhibit panel on its exhibit, its follow button (showing its type's idle picture seen from the south-east, centred and clipped, set when the panel opens and at a forced refresh such as a tab click) keeps the view on it, and a click
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
                      `panel guest none` when it closes. A click on a guest building (a burger stand, a restroom) opens its
                      panel: a building that charges has the price field with up and down buttons (0.25 a press, within the
                      economy's building price maximum and the type's low and high cost; the field of the full game can be
                      typed into, the number going through the same bounds), the income, upkeep and profit since built and per month, the months open
                      and the items sold (refreshed every 5 s of the clock); a type that hides the cost change (a grandstand) hides the buttons and disables the field, one that hides the commerce info leaves everything below the price unfilled and hides two tab buttons; both panels show the type's information image and the name field renames the building (a text with no letter, digit or punctuation mark keeps the name); any other building shows its guests and months open
                      (and on the full game its monthly upkeep), filled when it opens. `panel building ID \"NAME\" price P income I
                      upkeep U profit R month I U R months M guests last A this B total C sells [ITEMS]` tells what it shows, when
                      it opens and when a value changes (` fixed` follows the price when the buttons are hidden, and a panel with nothing filled below the price tells `price P`), `building price ID P` each change of the price, and
                      `panel building none` when it closes. The sell button of the panel sells the building as the bulldozer does (the
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
                      posts and `animal trample ID X,Y` for a tile it tramples; an animal in a tank adds `animal breath ID N` for the breath it gains under water (at its threshold it swims to the surface: the task `surfacebreath`) and `animal terrain ID MODE` when the water check changes its terrain mode (0 land, 1 water, 2 tank); an animal logs `animal likes ID` or `animal dislikes ID` when an object, an animal or a staff member placed, put down after a pick-up or removed, a terrain change or a new wall height of a filled tank with a gate in its exhibit raises or lowers its species' rating there (an animal or an object decides by the species' values, negated for a removal, when the rating stays the same), and shows a smiling or frowning face above itself for 1250 ms; the change then logs `animal sound smile` if an animal (not an egg) reacted with a like and `animal sound frown` if one reacted with a dislike, and a window plays `sounds/smile.wav` or `sounds/frown.wav`. The new guest check lets a guest in at the zoo entrance and logs `guest add ID adult|child` (the admission is booked); each status tick of a guest logs `guest status ID HAPPINESS HUNGER THIRST BATHROOM ENERGY`; a guest that goes to see an exhibit logs `guest view ID EXHIBIT` when it starts viewing and `guest donate ID EXHIBIT AMOUNT` when it leaves the viewing area with a donation (booked as income); a guest that decided to leave (or is told to) walks to the entrance and is removed there (`guest leave ID`). A guest logs `guest thought ID TEXT` for each thought (a string id) and `guest litter ID X,Y` when it drops a piece of litter on a tile. A keeper logs `staff go ID EXHIBIT` when he starts for the exhibit that needs him, `staff job ID JOB` at each job of his duties there (`PutFood`, `PuttingFood`, `HealAnimal`, `HealingAnimal`, `CleanDirt`, `CleaningDirt`, `none`) and `staff done ID EXHIBIT` when it is serviced; the food and the medicine he buys are booked as animal food and animal health. A maintenance worker logs `staff job ID JOB` (`TrashCan`, `EmptyTrash`, `TrashPile`, `SweepTrash`, `GoToFence`, `FixFence`, `none`) at each job of the task it takes from the shared pool and `world fence ID LIFE` for each fence piece it makes healthy. Each unit runs a behaviour set
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
                      their index (fullscreen asks the window to switch), --log writes the
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
                      `menu credits closed` when Exit closed them. The button that loads a saved game is disabled until saved games
                      come. The scenario and freeform buttons show their screens with the lists
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
                      `lastfile`. The lines are `menu game start LABEL FILE`; a file that cannot
                      be loaded is a question box with its message (`menu message TEXT`) and the
                      menu stays. In a game, Escape (with nothing to cancel; an open panel closes
                      first), the window being closed (`close` in a script) and the Main Menu and
                      Exit Game buttons of the options panel ask whether to save (`menu game ask
                      leave|exit`, Yes No Cancel; Yes cannot save yet and changes nothing, `menu
                      game save unavailable`): No stops the game and shows the menu again (`menu
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
ZT_DATA_DIR environment variable is used. window-smoke does not need it.";

enum CliError {
    Usage,
    Failed(Box<dyn Error>),
}

impl<E: Into<Box<dyn Error>>> From<E> for CliError {
    fn from(err: E) -> Self {
        CliError::Failed(err.into())
    }
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            // A closed standard error is not worth a panic, which `eprintln!` would raise.
            let _ = writeln!(io::stderr(), "{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(err)) => {
            let _ = writeln!(io::stderr(), "rustyzoo: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), CliError> {
    let mut data_dir = None;
    let mut words = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--data" {
            data_dir = Some(args.next().ok_or(CliError::Usage)?);
        } else {
            words.push(arg.into_string().map_err(|_| CliError::Usage)?);
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();

    match words.as_slice() {
        ["window-smoke"] => Ok(window_smoke()?),
        ["ls"] => list(open_game(data_dir)?, None),
        ["ls", pattern] => list(open_game(data_dir)?, Some(pattern)),
        ["cat", member] => {
            let bytes = open_game(data_dir)?.read(member)?;
            ignore_closed_pipe(io::stdout().lock().write_all(&bytes))
        }
        ["dump-strings", dll] => dump_strings(game_folder(data_dir)?, dll, None),
        ["dump-strings", dll, "--text", name] => {
            dump_strings(game_folder(data_dir)?, dll, Some(name))
        }
        ["strings"] => merged_strings(&game_folder(data_dir)?),
        ["types"] => types(&open_game(data_dir)?),
        ["check-config"] => check_config(&open_game(data_dir)?),
        ["check-behaviours"] => check_behaviours(&open_game(data_dir)?, None),
        ["check-behaviours", "--type", name] => check_behaviours(&open_game(data_dir)?, Some(name)),
        ["dump-sprite", member, dir] => dump_sprite(&open_game(data_dir)?, member, Path::new(dir)),
        ["check-sprites"] => check_sprites(&open_game(data_dir)?),
        ["which", member] => which(open_game(data_dir)?, member),
        ["movie-info", source] => movie_info(data_dir, source, None),
        ["movie-info", source, "--dump", k, dir] => {
            let k = k.parse().ok().filter(|k| *k > 0).ok_or(CliError::Usage)?;
            movie_info(data_dir, source, Some((k, Path::new(dir))))
        }
        ["map-info", member] => map_info(&open_game(data_dir)?, member),
        ["check-maps"] => check_maps(&open_game(data_dir)?),
        ["check-exhibits", map] => check_exhibits(&open_game(data_dir)?, map),
        ["check-rating", map] => check_rating(&open_game(data_dir)?, map),
        ["render-ani", member, dir] => {
            render_ani(&open_game(data_dir)?, member.to_string(), Path::new(dir))
        }
        ["render-ani", root, dir, "--anim", name] => render_ani(
            &open_game(data_dir)?,
            anim::resolve_path(root, name),
            Path::new(dir),
        ),
        ["check-bitmaps"] => check_bitmaps(&open_game(data_dir)?),
        ["check-wavs"] => check_wavs(&open_game(data_dir)?),
        ["check-music"] => check_music(&open_game(data_dir)?),
        ["check-anis"] => check_anis(&open_game(data_dir)?),
        ["text-width", text, flags @ ..] => textcmd::text_width(text, flags),
        ["draw-text", out, rest @ ..] => match rest.split_last() {
            Some((text, flags)) => textcmd::draw_text(out, text, flags),
            None => Err(CliError::Usage),
        },
        ["timing"] => timing(&open_game(data_dir)?),
        ["check-ground"] => check_ground(&open_game(data_dir)?),
        ["render-ground", key, out] => render_ground(&open_game(data_dir)?, key, Path::new(out)),
        ["render-map", member, out, flags @ ..] => {
            let (rotation, options) = render_flags(flags)?;
            render_map(&open_game(data_dir)?, member, out, rotation, &options)
        }
        ["check-layouts", flags @ ..] => layout::check_layouts(&open_game(data_dir)?, flags),
        ["dump-layout", flags @ ..] => layout::dump_layout(data_dir, flags),
        ["render-ui", out, flags @ ..] => layout::render_ui(data_dir, out, flags),
        ["run", flags @ ..] if flags.first().is_none_or(|w| w.starts_with("--")) => {
            run::run(&open_game(data_dir)?, None, flags)
        }
        ["run", member, flags @ ..] => run::run(&open_game(data_dir)?, Some(member), flags),
        ["find-path", member, flags @ ..] => {
            findcmd::find_path(&open_game(data_dir)?, member, flags)
        }
        ["sim", member, flags @ ..] => simcmd::sim(&open_game(data_dir)?, member, flags),
        ["check-render"] => check_render(&open_game(data_dir)?, 0),
        ["check-render", "--rotation", n] => check_render(
            &open_game(data_dir)?,
            n.parse().map_err(|_| CliError::Usage)?,
        ),
        _ => Err(CliError::Usage),
    }
}

/// The game folder given by `--data`, or else by `ZT_DATA_DIR`.
fn game_folder(data_dir: Option<OsString>) -> Result<PathBuf, CliError> {
    data_dir
        .or_else(|| std::env::var_os("ZT_DATA_DIR").filter(|dir| !dir.is_empty()))
        .map(PathBuf::from)
        .ok_or_else(|| "no game folder: pass --data <dir> or set ZT_DATA_DIR".into())
}

fn open_game(data_dir: Option<OsString>) -> Result<Vfs, CliError> {
    let mut vfs = Vfs::open(&game_folder(data_dir)?)?;
    for warning in vfs.warnings() {
        eprintln!("rustyzoo: warning: {warning}");
    }
    if let Some(dir) = userdir::dir()
        && let Err(e) = userdir::apply(&mut vfs, &dir)
    {
        eprintln!("rustyzoo: warning: settings of the user folder not used: {e}");
    }
    Ok(vfs)
}

fn which(vfs: Vfs, member: &str) -> Result<(), CliError> {
    let copies = vfs.which(member)?;
    if copies.is_empty() {
        return Err(format!("no such member: {member}").into());
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = copies
        .iter()
        .enumerate()
        .try_for_each(|(i, copy)| {
            let label = if i == 0 { "supplied by" } else { "shadowed" };
            let kind = if copy.in_archive {
                "archive"
            } else {
                "loose file"
            };
            let empty = if copy.size == 0 && !copy.in_archive {
                ", empty: reading fails"
            } else {
                ""
            };
            writeln!(
                out,
                "{label:<11} {kind} {} ({} bytes{empty})",
                escape(&copy.path.to_string_lossy()),
                copy.size
            )
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// Prints the string table and the `TEXT` entries of `dll`, a file of the install folder
/// whose name is matched ignoring letter case, or only the bytes of the `TEXT` entry `text`.
fn dump_strings(dir: PathBuf, dll: &str, text: Option<&str>) -> Result<(), CliError> {
    let path = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read folder {}: {e}", dir.display()))?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(dll))
                && entry.path().is_file()
        })
        .map(|entry| entry.path())
        .ok_or_else(|| format!("no file {dll} in {}", dir.display()))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if size > vfs::MAX_MEMBER_SIZE {
        return Err(format!("{} is too big for a resource DLL", path.display()).into());
    }
    let file = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let resources = pe::resources(&file).map_err(|e| format!("{dll}: {e}"))?;
    let texts = pe::texts(&resources);

    if let Some(name) = text {
        let (_, data) = texts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{dll} has no TEXT entry named {name}"))?;
        return ignore_closed_pipe(io::stdout().lock().write_all(data));
    }
    let strings = pe::strings(&resources).map_err(|e| format!("{dll}: {e}"))?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = strings
        .iter()
        .try_for_each(|(id, text)| writeln!(out, "{id}\t{}", escape(text)))
        .and_then(|()| {
            texts.iter().try_for_each(|(name, data)| {
                writeln!(out, "TEXT\t{}\t{}", escape(name), data.len())
            })
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// Prints the merged view of the `lang*.dll` libraries of the install folder.
fn merged_strings(dir: &Path) -> Result<(), CliError> {
    let tables = StringTables::open(dir)?;
    if tables.mixed_case() {
        let _ = writeln!(
            io::stderr(),
            "rustyzoo: warning: the lang*.dll names differ in letter case; they are ordered by their bytes"
        );
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = tables
        .libraries()
        .try_for_each(|name| writeln!(out, "LIB\t{name}"))
        .and_then(|()| {
            tables
                .strings()
                .iter()
                .try_for_each(|(id, (lib, text))| writeln!(out, "{id}\t{lib}\t{}", escape(text)))
        })
        .and_then(|()| {
            tables.texts().iter().try_for_each(|(name, (lib, data))| {
                writeln!(out, "TEXT\t{}\t{lib}\t{}", escape(name), data.len())
            })
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The text on one line: backslash, newline, carriage return and tab become `\\`, `\n`, `\r`
/// and `\t`, and any other control character becomes `\u{XXXX}`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn list(vfs: Vfs, pattern: Option<&str>) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = vfs
        .list(pattern)
        .iter()
        .try_for_each(|path| writeln!(out, "{}", escape(path)))
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The members written in the INI-like dialect (see `rustyzoo::config`).
const CONFIG_EXTENSIONS: [&str; 9] = [
    ".ai", ".cfg", ".ani", ".scn", ".lyt", ".uca", ".ucb", ".ucs", ".ini",
];

/// Parses every configuration member and reports what the original would ignore. Nothing in a
/// file is an error; only a member that cannot be read or is too large makes the command fail.
fn check_config(vfs: &Vfs) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failed = 0;
    let written = report_config(vfs, &mut out, &mut failed).and_then(|()| out.flush());
    ignore_closed_pipe(written)?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} configuration files could not be read").into()),
    }
}

fn report_config(vfs: &Vfs, out: &mut impl Write, failed: &mut usize) -> io::Result<()> {
    let (mut files, mut sections, mut keys, mut values) = (0, 0, 0, 0);
    let (mut empty, mut before, mut after) = (0, 0, 0);
    for path in vfs.list(None) {
        if !CONFIG_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        files += 1;
        let config = vfs
            .read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Config::parse(&bytes).map_err(|e| e.to_string()));
        let config = match config {
            Ok(config) => config,
            Err(message) => {
                *failed += 1;
                writeln!(out, "{path}: {message}")?;
                continue;
            }
        };
        for (_, section) in config.sections() {
            sections += 1;
            for key in section.keys() {
                keys += 1;
                values += section.get_all(key).len();
            }
        }
        if config.is_empty() {
            empty += 1;
            writeln!(out, "{path}: no section")?;
        }
        let (b, a) = (
            config.ignored_before_first_header,
            config.ignored_after_broken_header,
        );
        if b + a > 0 {
            writeln!(
                out,
                "{path}: ignored lines: {b} before the first header, {a} after a broken header"
            )?;
        }
        before += b;
        after += a;
    }
    writeln!(
        out,
        "{files} files, {sections} sections, {keys} keys, {values} values; \
         {empty} files without a section; ignored lines: {before} before the first header, \
         {after} after a broken header; {failed} unreadable",
        failed = *failed
    )
}

/// Prints one line per entity type, then the counts per class. Warnings go to standard error.
fn types(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    for warning in &catalogue.warnings {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
    }
    let name = |bytes: &[u8]| escape(&String::from_utf8_lossy(bytes));
    let mut report = Report::new();
    for ty in &catalogue.types {
        let [x, y, z] = ty.footprint;
        let member = ty.base_animation().and_then(|animation| animation.member);
        let exists = member.as_deref().is_some_and(|member| vfs.contains(member));
        let member = member.as_deref().unwrap_or("-");
        report.line(format_args!(
            "{}\t{}\t{}\t{}\t{x}x{y}x{z}\t{}\t{}",
            ty.class,
            name(&ty.type_name),
            name(&ty.subtype),
            ty.name_id,
            escape(member),
            if exists { "yes" } else { "no" },
        ));
    }
    let counts: Vec<String> = catalogue
        .counts()
        .iter()
        .map(|(class, count)| format!("{class} {count}"))
        .collect();
    report.line(format_args!(
        "{} types: {}",
        catalogue.types.len(),
        counts.join(", ")
    ));
    report.finish()
}

fn check_behaviours(vfs: &Vfs, type_name: Option<&str>) -> Result<(), CliError> {
    let mut report = Report::new();
    if let Some(wanted) = type_name {
        let catalogue = Catalogue::load(vfs)?;
        for warning in &catalogue.warnings {
            let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
        }
        let ty = catalogue
            .types
            .iter()
            .find(|ty| ty.type_name == wanted.as_bytes())
            .ok_or_else(|| format!("check-behaviours: no type named {}", wanted.escape_debug()))?;
        let Some(found) = catalogue.behaviours(ty) else {
            report.line(format_args!("no behaviour set or ambient table"));
            return report.finish();
        };
        for warning in &found.warnings {
            let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
        }
        for (table, name) in found.tables.iter().zip(behaviour::TABLES) {
            for row in table {
                report.line(format_args!("{name}\t{}\t{}\t{}", row.lo, row.hi, row.set));
            }
        }
        for (section, calls) in found.sets() {
            for (i, call) in calls.iter().enumerate() {
                report.line(format_args!(
                    "{}\t{}\t{}\t{:?}",
                    escape(&String::from_utf8_lossy(section)),
                    i + 1,
                    call.name,
                    call.op
                ));
            }
        }
        return report.finish();
    }
    let survey = behaviour::survey(vfs)?;
    for warning in &survey.warnings {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
    }
    let mut counts: Vec<_> = survey.counts.iter().collect();
    counts.sort_by_key(|&(name, count)| (std::cmp::Reverse(count), name));
    for (name, count) in counts {
        report.line(format_args!("{name}\t{count}"));
    }
    let Survey {
        files,
        sets,
        calls,
        tables,
        unknown,
        defects,
        ..
    } = survey;
    report.line(format_args!(
        "{files} files, {sets} sets, {calls} calls, {tables} tables; {unknown} calls of unknown names, {defects} calls with defective arguments"
    ));
    report.finish()
}

/// A reader that stops early (`| head`) is not an error of ours.
fn ignore_closed_pipe(result: io::Result<()>) -> Result<(), CliError> {
    match result {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
    }
}

/// Lines for standard output that keep the work going when the reader has gone away: after the
/// first failed write nothing more is written, and [`Report::finish`] treats a closed pipe like
/// `ls` and `cat` do.
struct Report {
    out: io::StdoutLock<'static>,
    written: io::Result<()>,
}

impl Report {
    fn new() -> Report {
        Report {
            out: io::stdout().lock(),
            written: Ok(()),
        }
    }

    fn line(&mut self, line: fmt::Arguments) {
        if self.written.is_ok() {
            self.written = writeln!(self.out, "{line}");
        }
    }

    fn finish(mut self) -> Result<(), CliError> {
        let flushed = self.out.flush();
        ignore_closed_pipe(self.written.and(flushed))
    }
}

/// The file of a frame in `dump-sprite`: the frame number only, never anything from the sprite.
fn frame_file_name(number: usize, shadow: bool, extra: bool) -> String {
    let shadow = if shadow { "-shadow" } else { "" };
    let extra = if extra { "-extra" } else { "" };
    format!("{number:03}{shadow}{extra}.png")
}

fn dump_sprite(vfs: &Vfs, member: &str, dir: &Path) -> Result<(), CliError> {
    let (sprite, palette, warning) = sprite::load(vfs, member)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{member}: {} frames{}, delay {} ms, palette {:?}",
        sprite.frames.len(),
        if sprite.extra.is_some() {
            " + 1 extra"
        } else {
            ""
        },
        sprite.delay_ms,
        sprite.palette,
    ));
    if let Some(warning) = warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
    for (number, frame) in sprite.all_frames().enumerate() {
        let name = frame_file_name(number, frame.shadow, number == sprite.frames.len());
        let empty = frame.width == 0 || frame.height == 0;
        report.line(format_args!(
            "  {name}: {}x{} x offset {} y offset {} unknown {}{}",
            frame.width,
            frame.height,
            frame.x_offset,
            frame.y_offset,
            frame.unknown,
            if empty { " (empty, no file)" } else { "" },
        ));
        if empty {
            continue;
        }
        let mut image = Framebuffer::new(frame.width.into(), frame.height.into());
        if frame.shadow {
            // A shadow frame only darkens what is behind it: show it on white.
            image.pixels.fill(255);
        }
        frame.draw(&palette, &mut image, 0, 0);
        let path = dir.join(&name);
        let file =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(file))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    report.finish()
}

/// Decodes every sprite file (a member whose last path component has no extension).
fn check_sprites(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut files, mut frames, mut shadows, mut extras, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    for member in vfs.list(None) {
        if !sprite::is_sprite_member(&member) {
            continue;
        }
        files += 1;
        match sprite::load(vfs, &member) {
            Ok((sprite, _, warning)) => {
                if let Some(warning) = warning {
                    warned += 1;
                    report.line(format_args!(
                        "warning {}: {}",
                        member.escape_debug(),
                        warning.escape_debug()
                    ));
                }
                frames += sprite.frames.len();
                shadows += sprite.all_frames().filter(|frame| frame.shadow).count();
                extras += usize::from(sprite.extra.is_some());
            }
            Err(err) => {
                failed += 1;
                // Paths come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} sprite files, {frames} frames, {shadows} shadow frames, \
         {extras} files with an extra frame, {failed} failed, {warned} with a warning"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {files} sprite files failed").into()),
    }
}

/// How many entities `map-info` lists one by one.
const MAP_INFO_ENTITIES: usize = 5;

fn load_map(vfs: &Vfs, member: &str) -> Result<Zoo, CliError> {
    let bytes = vfs.read(member)?;
    Ok(Zoo::parse(&bytes).map_err(|e| format!("{member}: {e}"))?)
}

/// Reads a movie from a file, or else from the game, decodes it, and prints what it holds.
fn movie_info(
    data_dir: Option<OsString>,
    source: &str,
    dump: Option<(usize, &Path)>,
) -> Result<(), CliError> {
    let bytes = if Path::new(source).is_file() {
        std::fs::read(source).map_err(|e| format!("cannot read {}: {e}", source.escape_debug()))?
    } else {
        open_game(data_dir)?.read(source)?
    };
    let movie = Movie::parse(bytes)?;
    let mut report = Report::new();
    report.line(format_args!("movie: {}", source.escape_debug()));
    report.line(format_args!("size: {}x{}", movie.size().0, movie.size().1));
    report.line(format_args!("frames: {}", movie.frame_count()));
    let (rate, scale) = movie.frame_rate();
    report.line(format_args!(
        "frame rate: {rate}/{scale} ({:.2} per second)",
        f64::from(rate) / f64::from(scale)
    ));
    report.line(format_args!("duration: {} ms", movie.duration_ms()));
    match movie.audio_format() {
        Some(a) => report.line(format_args!(
            "audio: {} Hz, {} channels, {} bits, {} bytes",
            a.rate,
            a.channels,
            a.bits,
            movie.audio().len()
        )),
        None => report.line(format_args!("audio: none")),
    }
    if let Some((_, dir)) = dump {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let (mut decoded, mut dumped) = (0, 0);
    for (number, frame) in movie.frames().enumerate() {
        let frame = frame.map_err(|e| format!("frame {number}: {e}"))?;
        decoded += 1;
        if let Some((_, dir)) = dump.filter(|(k, _)| number % k == 0) {
            let path = dir.join(format!("frame-{number:06}.png"));
            let file = File::create(&path)
                .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
            write_png(&frame, io::BufWriter::new(file))
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            dumped += 1;
        }
    }
    report.line(format_args!("decoded: {decoded} frames"));
    if dump.is_some() {
        report.line(format_args!("dumped: {dumped} pictures"));
    }
    report.finish()
}

fn map_info(vfs: &Vfs, member: &str) -> Result<(), CliError> {
    let zoo = load_map(vfs, member)?;
    let mut report = Report::new();
    report.line(format_args!("member: {}", member.escape_debug()));
    report.line(format_args!("version: {}", zoo.version));
    report.line(format_args!("locale: {}", zoo.locale));
    report.line(format_args!("expansion: {}", zoo.expansion));
    report.line(format_args!("map size: {}x{}", zoo.width, zoo.height));
    report.line(format_args!(
        "entrance: {},{}",
        zoo.entrance.0, zoo.entrance.1
    ));
    report.line(format_args!("habitats: {}", zoo.habitat_tiles.len()));
    report.line(format_args!("id counter: {}", zoo.id_counter));
    report.line(format_args!("tiles: {}", zoo.tiles.len()));
    let mut terrain = BTreeMap::new();
    for tile in &zoo.tiles {
        *terrain.entry(tile.terrain).or_insert(0usize) += 1;
    }
    for (kind, count) in terrain {
        report.line(format_args!("  terrain {kind}: {count}"));
    }
    let skipped = zoo.entities.iter().filter(|e| e.common.is_none()).count();
    report.line(format_args!(
        "entities: {} ({skipped} without a decoded body)",
        zoo.entities.len()
    ));
    let mut classes = BTreeMap::new();
    for entity in &zoo.entities {
        *classes.entry(entity.class.as_str()).or_insert(0usize) += 1;
    }
    for (class, count) in classes {
        report.line(format_args!("  class {}: {count}", escape(class)));
    }
    for entity in zoo.entities.iter().take(MAP_INFO_ENTITIES) {
        let names = format!(
            "{}/{}/{}",
            escape(&entity.class),
            escape(&entity.type_name),
            escape(&entity.subtype)
        );
        match &entity.common {
            Some(c) => report.line(format_args!(
                "  {names} tile {},{} position {},{},{} rotation {} id {} name \"{}\"",
                c.x.div_euclid(64),
                c.y.div_euclid(64),
                c.x,
                c.y,
                c.z,
                c.rotation,
                c.id,
                escape(&c.name)
            )),
            None => report.line(format_args!("  {names} (body skipped)")),
        }
    }
    report.finish()
}

/// Reads every `.zoo` member. A file the original refuses by its header (not a saved game, a
/// version below 38 or above 106) is a refusal, as expected from leftovers; anything else that
/// fails, including a version below 70 with an entity of an undefined type, is a corrupted
/// file, which makes the command fail.
fn check_maps(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let mut report = Report::new();
    let (mut files, mut ok, mut refused, mut corrupted) = (0, 0, 0, 0);
    let (mut tiles, mut entities) = (0, 0);
    for member in vfs.list(None) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        files += 1;
        let name = member.escape_debug();
        let result = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        match result {
            Ok(zoo) => {
                ok += 1;
                tiles += zoo.tiles.len();
                entities += zoo.entities.len();
                report.line(format_args!(
                    "ok {name}: version {}, {}x{}, {} tiles, {} entities",
                    zoo.version,
                    zoo.width,
                    zoo.height,
                    zoo.tiles.len(),
                    zoo.entities.len()
                ));
            }
            Err(err) => {
                match err {
                    ZooError::Corrupted(_) => corrupted += 1,
                    _ => refused += 1,
                }
                // The text comes from the file system: keep it on one line.
                report.line(format_args!(
                    "FAILED {name}: {}",
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} map files, {ok} ok, {refused} refused by version or magic, \
         {corrupted} corrupted; {tiles} tiles, {entities} entities in the ok files"
    ));
    report.finish()?;
    match corrupted {
        0 => Ok(()),
        _ => Err(format!("{corrupted} of {files} map files are corrupted").into()),
    }
}

/// Finds the regions of the maps that match `pattern` (a member name, or a pattern with `*` as
/// for `ls`) from their fences and compares the number of exhibits with the habitat count the file
/// stores, and checks that each stored first tile lies in a different exhibit; the same for the
/// tanks. A mismatch is listed and fails the command.
fn check_exhibits(vfs: &Vfs, pattern: &str) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let mut report = Report::new();
    let (mut maps, mut agree, mut mismatch, mut skipped) = (0, 0, 0, 0);
    for member in vfs.list(Some(pattern)) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        maps += 1;
        let name = member.escape_debug();
        let zoo = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        let zoo = match zoo {
            Ok(zoo) => zoo,
            Err(err) => {
                skipped += 1;
                report.line(format_args!(
                    "skipped {name}: {}",
                    err.to_string().escape_debug()
                ));
                continue;
            }
        };
        let stored = zoo.habitat_tiles.len();
        let tiles = zoo.habitat_tiles.clone();
        let stored_tanks = zoo.tanks.clone();
        let world = World::new(zoo, |e| renderer.shape(e));
        let list = world.exhibits().list();
        let distinct = world.exhibits().distinct(&world, &tiles);
        let sizes: Vec<String> = list.iter().map(|e| e.tiles.to_string()).collect();
        // The tanks found from the walls are the stored ones: as many, and each stored tank's
        // first tile lies in one.
        let tanks = list.iter().filter(|e| e.tank.is_some()).count();
        let tank_tiles = stored_tanks
            .iter()
            .filter(|t| {
                world
                    .tile_index(t.tile.0, t.tile.1)
                    .is_some_and(|i| world.tank_at(i).is_some())
            })
            .count();
        let stored_tanks = stored_tanks.len();
        let verdict = if list.len() == stored
            && distinct == stored
            && tanks == stored_tanks
            && tank_tiles == stored_tanks
        {
            agree += 1;
            "ok"
        } else {
            mismatch += 1;
            "MISMATCH"
        };
        report.line(format_args!(
            "{verdict} {name}: {} exhibits, {stored} stored, {distinct} of {stored} stored first \
             tiles in distinct exhibits; {tanks} tanks, {stored_tanks} stored, {tank_tiles} of \
             {stored_tanks} stored tank tiles in a tank; sizes [{}]",
            list.len(),
            sizes.join(" ")
        ));
    }
    report.line(format_args!(
        "{maps} maps, {agree} agree, {mismatch} differ, {skipped} skipped"
    ));
    report.finish()?;
    match (maps, mismatch) {
        (0, _) => Err(format!("check-exhibits: no map matches {}", pattern.escape_debug()).into()),
        (_, 0) => Ok(()),
        _ => {
            Err(format!("{mismatch} of {maps} maps differ from their stored habitat count").into())
        }
    }
}

/// Rates every exhibit of the maps matching `pattern` for each species in it (habitat rating).
fn check_rating(vfs: &Vfs, pattern: &str) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let renderer = MapRenderer::new(vfs, &catalogue);
    let mut report = Report::new();
    let (mut maps, mut rated, mut bad) = (0, 0, 0);
    for member in vfs.list(Some(pattern)) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        let zoo = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| zoo.checked(&catalogue));
        let Ok(zoo) = zoo else {
            report.line(format_args!("skipped {}", member.escape_debug()));
            continue;
        };
        maps += 1;
        let world = World::new(zoo, |e| renderer.shape(e));
        let ground = viewing::Ground::of(&world, viewing::bodies(&catalogue));
        let wear = viewing::wear(&world, viewing::decayed(&catalogue));
        for exhibit in world.exhibits().list() {
            let input = rating::input(&world, &catalogue, exhibit.id);
            let mut state = rating::State::new();
            state.compute(&input);
            report.line(format_args!(
                "{} exhibit {}{}: {} tiles, {} animals, attractiveness {}",
                member.escape_debug(),
                exhibit.id,
                if exhibit.tank.is_some() {
                    " (tank)"
                } else {
                    ""
                },
                exhibit.tiles,
                input.animals.len(),
                state.attractiveness,
            ));
            let plan = viewing::Plan::of(&world, exhibit.id);
            let mut areas = viewing::areas(&plan);
            report.line(format_args!(
                "  {} viewing areas, deterioration {:?}",
                areas.len(),
                wear.get(&exhibit.id).copied().unwrap_or_default(),
            ));
            for (n, area) in areas.iter_mut().enumerate() {
                area.observe(&plan, &ground);
                let (xs, ys): (Vec<_>, Vec<_>) = (area.tiles.iter())
                    .map(|&t| (t % plan.width, t / plan.width))
                    .unzip();
                let span =
                    |v: &[usize]| v.iter().max().unwrap_or(&0) - v.iter().min().unwrap_or(&0) + 1;
                // An area covers at most 4 columns and 4 rows.
                bad += usize::from(span(&xs) > 4 || span(&ys) > 4);
                report.line(format_args!(
                    "    area {n}: {} tiles from ({}, {}), {} columns x {} rows, sees {} tiles",
                    area.tiles.len(),
                    xs[0],
                    ys[0],
                    span(&xs),
                    span(&ys),
                    area.seen.len(),
                ));
            }
            for r in &state.records {
                rated += 1;
                let bits: Vec<String> = (0..64)
                    .filter(|&b| r.has(b))
                    .map(|b| format!("{b}{}", if r.important >> b & 1 == 1 { "!" } else { "" }))
                    .collect();
                if r.shown() > 100 {
                    bad += 1;
                }
                report.line(format_args!(
                    "  {} x{}: shown {} (rating {:.2}) terrain {} objects {} foliage {} rocks {} \
                     elevation {} shelters {} toys {} depth {} water {}; bits [{}]",
                    r.key.escape_debug(),
                    r.animals,
                    r.shown(),
                    r.rating(),
                    r.terrain,
                    r.objects,
                    r.foliage,
                    r.rocks,
                    r.elevation,
                    r.shelters,
                    r.toys,
                    r.depth,
                    r.water,
                    bits.join(" "),
                ));
            }
        }
    }
    report.line(format_args!(
        "{maps} maps, {rated} species rated, {bad} out of range"
    ));
    report.finish()?;
    match (maps, bad) {
        (0, _) => Err(format!("check-rating: no map matches {}", pattern.escape_debug()).into()),
        (_, 0) => Ok(()),
        _ => Err(
            format!("{bad} ratings are not 0 to 100 or areas cover more than 4 x 4 tiles").into(),
        ),
    }
}

/// The names of the eight screen directions, in the order of the numbers 0 to 7.
const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// The largest width or height of the canvas of `render-ani`.
const MAX_CANVAS: i64 = sprite::MAX_DIMENSION as i64;

/// Draws frame 0 of the set for each of the eight screen directions on a transparent canvas
/// that holds the box of the set, the box of its frames and their mirror images, with the
/// anchor at (`m`, `top`) where `m` is the largest distance of the box from the anchor to the
/// left or right and `top` its distance above.
fn render_ani(vfs: &Vfs, member: String, dir: &Path) -> Result<(), CliError> {
    let set = AnimSet::load(vfs, &member)?;
    let animation = Animation::open(vfs, set).map_err(|(err, _)| err)?;
    let set = &animation.set;
    let [e0, f0, e1, f1] = animation.extents().unwrap_or_default();
    let (x0, y0, x1, y1) = (
        i64::from(set.x0.min(e0)),
        i64::from(set.y0.min(f0)),
        i64::from(set.x1.max(e1)),
        i64::from(set.y1.max(f1)),
    );
    let reach = x0.abs().max(x1.abs());
    let (anchor_x, anchor_y) = (reach, (-y0).max(0));
    let (width, height) = (2 * reach + 1, (anchor_y + y1.max(0)).max(1));
    if width > MAX_CANVAS || height > MAX_CANVAS {
        return Err(format!("the box {x0} {y0} {x1} {y1} is too large to draw").into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{}: {} files, box {x0} {y0} {x1} {y1}; canvas {width}x{height}, anchor ({anchor_x}, {anchor_y})",
        member.escape_debug(),
        animation.sprites.len(),
    ));
    if !matches!(animation.sprites.len(), 1 | 4 | 5) {
        report.line(format_args!(
            "note: a set of {} files is not chosen by screen direction; the images show the rule for it anyway",
            animation.sprites.len()
        ));
    }
    if let Some(warning) = &animation.warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
    // A set of shadow frames only darkens what is behind it: show it on white.
    let on_white = animation
        .sprites
        .iter()
        .any(|loaded| loaded.0.frames.first().is_some_and(|frame| frame.shadow));
    for (direction, name) in DIRECTIONS.iter().enumerate() {
        let view = animation.view(direction as u8);
        let mut image = Framebuffer::new(width as u32, height as u32);
        if on_white {
            image.pixels.fill(255);
        }
        animation.draw(view, 0, &mut image, anchor_x as i32, anchor_y as i32);
        let file = format!("dir{direction}-{name}.png");
        let path = dir.join(&file);
        let out =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(out))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        report.line(format_args!(
            "  {file}: file {} ({}){}",
            view.file,
            animation.set.files[view.file]
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .escape_debug(),
            if view.mirror { ", mirrored" } else { "" },
        ));
    }
    report.finish()
}

/// Loads every `.ani` member and the sprite files it names. A set that cannot be read, or one
/// with a sprite file that cannot be found, fails, as in the original. A defective palette is a
/// warning: the set loads.
fn check_anis(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut sets, mut files, mut missing, mut mirrored, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    let mut sizes = std::collections::BTreeMap::new();
    let (mut exact, mut larger, mut different) = (0, 0, 0);
    for member in vfs.list(None) {
        if !member.ends_with(".ani") {
            continue;
        }
        sets += 1;
        let result = AnimSet::load(vfs, &member).and_then(|set| {
            files += set.files.len();
            *sizes.entry(set.files.len()).or_insert(0) += 1;
            mirrored += (0..8)
                .filter(|&u| anim::view_for(set.files.len(), u).mirror)
                .count();
            Animation::open(vfs, set).map_err(|(err, lost)| {
                missing += lost;
                err
            })
        });
        match result {
            Ok(animation) => {
                let set = &animation.set;
                let boxed = [set.x0, set.y0, set.x1, set.y1];
                let extents = animation.extents().unwrap_or_default();
                let holds = boxed[0] <= extents[0]
                    && boxed[1] <= extents[1]
                    && boxed[2] >= extents[2]
                    && boxed[3] >= extents[3];
                match (boxed == extents, holds) {
                    (true, _) => exact += 1,
                    (_, true) => larger += 1,
                    _ => different += 1,
                }
                match &animation.warning {
                    Some(warning) => {
                        warned += 1;
                        report.line(format_args!(
                            "warning {}: {}",
                            member.escape_debug(),
                            warning.escape_debug()
                        ));
                    }
                    None => report.line(format_args!("ok {}", member.escape_debug())),
                }
            }
            Err(err) => {
                failed += 1;
                // Names come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    let sizes: Vec<String> = sizes.iter().map(|(n, c)| format!("{n}:{c}")).collect();
    report.line(format_args!(
        "{sets} animation sets, {files} sprite file references, {missing} missing files, \
         {mirrored} mirrored views by the file-count rule, {failed} failed, \
         {warned} with a warning; sets by file count {}; \
         box: {exact} equal to the frame extents, {larger} larger, {different} different",
        sizes.join(" ")
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {sets} animation sets failed").into()),
    }
}

/// Differences between the rendered and the cached pictures of one terrain type.
struct GroundStats {
    pictures: usize,
    /// Pictures where some pixel is covered in only one of the two.
    differing: usize,
    /// Pixels covered in only one of the two.
    uncommon: usize,
    /// Pictures whose cut differs although the covered pixels do not.
    boxes: usize,
    /// Pixels covered in both, and the sum of the absolute differences per channel.
    covered: u64,
    sums: [u64; 3],
    /// How many (pixel, channel) pairs differ by each amount.
    histogram: Vec<u64>,
}

impl GroundStats {
    fn new() -> GroundStats {
        GroundStats {
            pictures: 0,
            differing: 0,
            uncommon: 0,
            boxes: 0,
            covered: 0,
            sums: [0; 3],
            histogram: vec![0; 256],
        }
    }

    fn add(&mut self, other: &GroundStats) {
        self.pictures += other.pictures;
        self.differing += other.differing;
        self.uncommon += other.uncommon;
        self.boxes += other.boxes;
        self.covered += other.covered;
        for c in 0..3 {
            self.sums[c] += other.sums[c];
        }
        for (a, b) in self.histogram.iter_mut().zip(&other.histogram) {
            *a += b;
        }
    }

    /// The smallest difference that at least `percent` of the (pixel, channel) pairs do not exceed.
    fn percentile(&self, percent: u64) -> usize {
        let total: u64 = self.histogram.iter().sum();
        let mut seen = 0;
        for (value, count) in self.histogram.iter().enumerate() {
            seen += count;
            if seen * 100 >= total * percent {
                return value;
            }
        }
        0
    }

    /// The counts, then after a semicolon the colour differences. `extra` goes at the end of the
    /// counts.
    fn describe(&self, extra: &str) -> String {
        let mean = self.sums.map(|s| s as f64 / self.covered.max(1) as f64);
        format!(
            "{} pictures, {} covered pixels, {} pictures with differing covered pixels, \
             {} differing pixels, {} pictures with another cut{extra}; colour difference per \
             channel over covered pixels: mean R {:.2} G {:.2} B {:.2}, 95th percentile {}, max {}",
            self.pictures,
            self.covered,
            self.differing,
            self.uncommon,
            self.boxes,
            mean[0],
            mean[1],
            mean[2],
            self.percentile(95),
            self.histogram.iter().rposition(|&n| n > 0).unwrap_or(0),
        )
    }
}

/// Renders every picture of the terrain cache again and compares the pixels both cover.
fn check_ground(vfs: &Vfs) -> Result<(), CliError> {
    let renderer = Renderer::load(vfs)?;
    let mut report = Report::new();
    let mut per_type: BTreeMap<(u8, bool), GroundStats> = Default::default();
    let mut failed = 0;
    let members = [vfs.list(Some("ztats/*.lle")), vfs.list(Some("ztatb/*.lle"))].concat();
    let palettes: HashSet<String> = [vfs.list(Some("ztats/*.pal")), vfs.list(Some("ztatb/*.pal"))]
        .concat()
        .into_iter()
        .collect();
    let (mut plain, mut blended) = (GroundStats::new(), GroundStats::new());
    let mut unusable = 0;
    for member in members {
        // The original renders a picture whose palette is missing, so there is nothing to compare.
        if Key::parse(&member).is_ok_and(|key| !palettes.contains(&key.palette())) {
            unusable += 1;
            continue;
        }
        let compared = ground::load_cached(vfs, &member).and_then(|(key, cached)| {
            let rendered = renderer.render(&key)?;
            Ok((key, cached, rendered))
        });
        let (key, cached, rendered) = match compared {
            Ok(compared) => compared,
            Err(err) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
                continue;
            }
        };
        let mut one = GroundStats::new();
        one.pictures = 1;
        // The cached picture is drawn, so it is as the screen shows it; so is the rendered one.
        let (a, mut b) = (cached.on_canvas(), rendered.on_canvas());
        for pixel in b.pixels.as_chunks_mut::<4>().0 {
            let [r, g, bl] = rustyzoo::sprite::screen([pixel[0], pixel[1], pixel[2]]);
            pixel[..3].copy_from_slice(&[r, g, bl]);
        }
        for (a, b) in a
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.pixels.as_chunks::<4>().0)
        {
            match (a[3] != 0, b[3] != 0) {
                (true, true) => {
                    one.covered += 1;
                    for c in 0..3 {
                        let difference = a[c].abs_diff(b[c]);
                        one.sums[c] += u64::from(difference);
                        one.histogram[usize::from(difference)] += 1;
                    }
                }
                (false, false) => {}
                _ => one.uncommon += 1,
            }
        }
        if one.uncommon > 0 {
            one.differing = 1;
            report.line(format_args!("DIFFERS {member}: {} pixels", one.uncommon));
        } else if (
            cached.image.width,
            cached.image.height,
            cached.x_offset,
            cached.y_offset,
        ) != (
            rendered.image.width,
            rendered.image.height,
            rendered.x_offset,
            rendered.y_offset,
        ) {
            one.boxes = 1;
            report.line(format_args!("CUT {member}"));
        }
        per_type
            .entry((key.terrain, key.is_uniform()))
            .or_insert_with(GroundStats::new)
            .add(&one);
        if key.is_uniform() {
            &mut plain
        } else {
            &mut blended
        }
        .add(&one);
    }
    let mut total = GroundStats::new();
    for ((terrain, uniform), stats) in &per_type {
        let kind = if *uniform { "plain" } else { "blended" };
        report.line(format_args!(
            "type {terrain:02x} {kind}: {}",
            stats.describe("")
        ));
        total.add(stats);
    }
    report.line(format_args!(
        "plain (ztats, lit in the cache, rendered unlit): {}",
        plain.describe("")
    ));
    report.line(format_args!("blended (ztatb): {}", blended.describe("")));
    report.line(format_args!(
        "total: {}",
        total.describe(&format!(", {unusable} without palette, {failed} failed"))
    ));
    report.finish()?;
    if total.uncommon > 0 || total.boxes > 0 || failed > 0 {
        return Err(format!(
            "{} pictures differ in covered pixels, {} in their cut, {failed} failed",
            total.differing, total.boxes
        )
        .into());
    }
    Ok(())
}

fn render_ground(vfs: &Vfs, key: &str, out: &Path) -> Result<(), CliError> {
    let key = Key::parse(key)?;
    let picture = Renderer::load(vfs)?.render(&key)?;
    if picture.image.width == 0 {
        return Err(format!("{} draws nothing", key.member()).into());
    }
    let file = File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    write_png(&picture.image, io::BufWriter::new(file))
        .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!(
        "{}: {}x{} x offset {} y offset {}",
        key.member(),
        picture.image.width,
        picture.image.height,
        picture.x_offset,
        picture.y_offset
    );
    Ok(())
}

/// The rotation and the options of `render-map`, from the words after the output file.
fn render_flags(mut flags: &[&str]) -> Result<(u8, Options), CliError> {
    let (mut rotation, mut options) = (0, Options::default());
    let number = |word: &&str| word.parse::<i32>().map_err(|_| CliError::Usage);
    while let [flag, rest @ ..] = flags {
        flags = match (*flag, rest) {
            ("--rotation", [n, rest @ ..]) => {
                rotation = n.parse().map_err(|_| CliError::Usage)?;
                rest
            }
            ("--view", [x, y, w, h, rest @ ..]) => {
                let size = |word: &&str| {
                    u32::try_from(number(word)?)
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or(CliError::Usage)
                };
                options.window = Some((number(x)?, number(y)?, size(w)?, size(h)?));
                rest
            }
            ("--no-units", rest) => {
                options.no_units = true;
                rest
            }
            ("--grid", rest) => {
                options.grid = true;
                rest
            }
            ("--zoom", [z, rest @ ..]) => {
                options.zoom = number(z)?;
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    Ok((rotation, options))
}

/// Draws one map into a PNG file and prints the summary.
fn render_map(
    vfs: &Vfs,
    member: &str,
    out: &str,
    rotation: u8,
    options: &Options,
) -> Result<(), CliError> {
    if rotation > 3 {
        return Err(CliError::Usage);
    }
    let catalogue = Catalogue::load(vfs)?;
    let zoo = load_map(vfs, member)?
        .checked(&catalogue)
        .map_err(|e| format!("{member}: {e}"))?;
    let (image, summary) =
        MapRenderer::new(vfs, &catalogue).render_with(&zoo, rotation, options)?;
    let file = File::create(out).map_err(|e| format!("cannot create {out}: {e}"))?;
    write_png(&image, io::BufWriter::new(file)).map_err(|e| format!("cannot write {out}: {e}"))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{} -> {}x{} pixels: {summary}",
        member.escape_debug(),
        image.width,
        image.height
    ));
    report.finish()
}

/// Draws every map in memory and prints the summaries. A file the loader refuses is skipped,
/// as `check-maps` lists it; only a map that cannot be drawn fails the command.
fn check_render(vfs: &Vfs, rotation: u8) -> Result<(), CliError> {
    if rotation > 3 {
        return Err(CliError::Usage);
    }
    let catalogue = Catalogue::load(vfs)?;
    let mut renderer = MapRenderer::new(vfs, &catalogue);
    let mut report = Report::new();
    let (mut total, mut maps, mut skipped, mut failed) = (Summary::default(), 0, 0, 0);
    for member in vfs.list(None) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        let name = member.escape_debug();
        let zoo = vfs
            .read(&member)
            .ok()
            .and_then(|bytes| Zoo::parse(&bytes).ok())
            .and_then(|zoo| zoo.checked(&catalogue).ok());
        let Some(zoo) = zoo else {
            skipped += 1;
            report.line(format_args!("skipped {name}: cannot be loaded"));
            continue;
        };
        match renderer
            .stage(&zoo, rotation, false)
            .and_then(|staged| staged.size().map(|_| staged.summary))
        {
            Ok(summary) => {
                maps += 1;
                report.line(format_args!("ok {name}: {summary}"));
                total.add(&summary);
            }
            Err(err) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {name}: {}",
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{maps} maps drawn, {skipped} skipped, {failed} failed; totals: {total}"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} maps could not be drawn").into()),
    }
}

/// The loop settings of the ini file, and a simulated run with a pass at every millisecond.
fn timing(vfs: &Vfs) -> Result<(), CliError> {
    let mut timers = Timers::from_config(&Config::parse(vfs.ini())?);
    let mut out = io::stdout().lock();
    let first = writeln!(
        out,
        "periods: update {} ms, draw {} ms, cap {} ms",
        timers.update_period(),
        timers.draw_period(),
        timers.cap()
    );
    ignore_closed_pipe(first)?;
    timers.pass(0, 1.0);
    let (mut draws, mut interfaces, mut steps) = (0, 0, 0);
    for now in 1..=10_000 {
        let pass = timers.pass(now, 1.0);
        draws += u32::from(pass.draw);
        interfaces += u32::from(pass.interface.is_some());
        steps += u32::from(pass.step.is_some());
    }
    ignore_closed_pipe(writeln!(
        out,
        "ten seconds, clock of 1 ms: {draws} draws, {interfaces} interface updates, {steps} steps"
    ))
}

/// Converts every root bitmap of the terrain and compares it with its shipped copy.
fn check_bitmaps(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut same, mut differ, mut alone, mut failed) = (0, 0, 0, 0);
    let names = bitmap::names(vfs);
    for name in &names {
        let converted = vfs
            .read(name)
            .map_err(|e| e.to_string())
            .and_then(|bytes| bitmap::convert(&bytes));
        let shipped = bitmap::shipped(vfs, name);
        let name = name.escape_debug();
        match (converted, shipped) {
            (Err(why), _) => {
                failed += 1;
                report.line(format_args!("FAILED {name}: {}", why.escape_debug()));
            }
            (Ok(_), None) => {
                alone += 1;
                report.line(format_args!("converted {name}: no shipped copy"));
            }
            (Ok(ours), Some(theirs)) => match bitmap::compare(&ours, &theirs) {
                Ok(()) => {
                    same += 1;
                    report.line(format_args!("identical {name}"));
                }
                Err(why) => {
                    differ += 1;
                    report.line(format_args!("DIFFERS {name}: {why}"));
                }
            },
        }
    }
    report.line(format_args!(
        "{} bitmaps: {same} identical to the shipped copy, {differ} differ, \
         {alone} without a shipped copy, {failed} failed",
        names.len()
    ));
    report.finish()?;
    match differ + failed {
        0 => Ok(()),
        n => Err(format!("{n} bitmaps differ or failed").into()),
    }
}

fn check_wavs(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let mut formats = BTreeMap::new();
    let mut failed = 0;
    let names: Vec<String> = vfs
        .list(None)
        .into_iter()
        .filter(|name| name.ends_with(".wav"))
        .collect();
    for name in &names {
        let sound = vfs
            .read(name)
            .map_err(|e| e.to_string())
            .and_then(|bytes| audio::Sound::decode(&bytes).map_err(|e| e.to_string()));
        match sound {
            Ok(sound) => *formats.entry(sound.describe()).or_insert(0) += 1,
            Err(why) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {}: {}",
                    name.escape_debug(),
                    why.escape_debug()
                ));
            }
        }
    }
    for (format, count) in &formats {
        report.line(format_args!("{count} {format}"));
    }
    report.line(format_args!(
        "{} wavs: {} ok, {failed} failed",
        names.len(),
        names.len() - failed
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} wavs failed").into()),
    }
}

fn check_music(vfs: &Vfs) -> Result<(), CliError> {
    let audio = audio::Audio::open(&sdl3::init()?.audio()?)?;
    let Some(model) = Music::from_config(&Config::parse(vfs.ini())?) else {
        println!("no menu music");
        return Ok(());
    };
    let Some(music) = audio.menu_music(vfs, &model, true, true)? else {
        println!("no menu music");
        return Ok(());
    };
    println!(
        "{}: {}, gain {:.3}, queued {} bytes, paused",
        model.file(),
        music.sound().describe(),
        music.gain()?,
        music.queued_bytes()?
    );
    // Exercises the rest of the interface; the device is dummy, so nothing is heard.
    music.resume()?;
    music.pause()?;
    Ok(())
}

fn window_smoke() -> Result<(), Box<dyn Error>> {
    let mut framebuffer = Framebuffer::new(800, 600);
    framebuffer.fill_test_pattern();

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let audio = sdl.audio()?;

    let window = video
        .window("RustyZoo", framebuffer.width, framebuffer.height)
        .build()?;
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let textures = canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(
        PixelFormat::RGBA32,
        framebuffer.width,
        framebuffer.height,
    )?;
    texture.update(None, &framebuffer.pixels, framebuffer.width as usize * 4)?;
    canvas.copy(&texture, None, None)?;
    if !canvas.present() {
        return Err(sdl3::get_error().into());
    }

    sdl.event_pump()?.pump_events();
    let _device = audio.open_playback_device(&AudioSpec::default())?;

    println!("SDL version:  {}", sdl3::version::version());
    println!("video driver: {}", video.current_video_driver());
    println!("audio driver: {}", audio.current_audio_driver());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{escape, frame_file_name, render_flags};

    #[test]
    fn frame_files_are_named_by_number_shadow_and_extra() {
        assert_eq!(frame_file_name(0, false, false), "000.png");
        assert_eq!(frame_file_name(12, true, false), "012-shadow.png");
        assert_eq!(frame_file_name(1234, false, true), "1234-extra.png");
        assert_eq!(frame_file_name(21, true, true), "021-shadow-extra.png");
    }

    #[test]
    fn escape_keeps_a_text_on_one_line() {
        assert_eq!(escape("a\\b\n\tc\rd"), "a\\\\b\\n\\tc\\rd");
        assert_eq!(
            escape("\0\u{1b}\u{7f}\u{85}é"),
            "\\u{0000}\\u{001b}\\u{007f}\\u{0085}é"
        );
    }

    #[test]
    fn render_map_flags_come_in_any_order_and_bad_ones_are_refused() {
        let ok = |flags: &[&str]| {
            render_flags(flags)
                .ok()
                .map(|(r, o)| (r, o.window, o.no_units, o.zoom))
        };
        assert_eq!(ok(&[]), Some((0, None, false, 0)));
        assert_eq!(ok(&["--zoom", "-2"]), Some((0, None, false, -2)));
        let grid = |flags: &[&str]| render_flags(flags).map(|(_, o)| o.grid).ok();
        assert_eq!(grid(&[]), Some(false));
        assert_eq!(grid(&["--grid", "--zoom", "2"]), Some(true));
        assert_eq!(grid(&["--grid", "1"]), None);
        assert_eq!(
            ok(&[
                "--no-units",
                "--view",
                "-5",
                "7",
                "30",
                "20",
                "--rotation",
                "2"
            ]),
            Some((2, Some((-5, 7, 30, 20)), true, 0))
        );
        for bad in [
            &["--view", "1", "2", "3"][..],
            &["--view", "1", "2", "-3", "4"],
            &["--view", "1", "2", "0", "4"],
            &["--view", "1", "2", "3", "0"],
            &["--view", "1", "2", "3", "x"],
            &["--rotation"],
            &["--zoom"],
            &["--zoom", "x"],
            &["--unit"],
        ] {
            assert!(ok(bad).is_none(), "{bad:?}");
        }
    }
}
