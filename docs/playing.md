# Playing a freeform game

How a person plays, in the window, what the two scripts of `ci/` play without one: `ci/playable.txt` on the base trial, `ci/playable-tank.txt` on the expansion trial. Each script is the list of the mouse moves, clicks and keys of the walk-through below, one per line with the number of the pass it acts in, and `ci/check-playable.sh` plays it twice and reads the log. The positions in the scripts are pixels of the default window (1024x768); in a window of another size the panels sit elsewhere.

Open a game folder with `--data <folder>` (or `ZT_DATA_DIR`), and start the game with `rustyzoo --data <folder> run`. The game starts up to the main menu.

## Start

1. At the main menu press the freeform button. A list of maps comes up with the starting cash beside it. Choose a map (the first is chosen) and press Play. The map opens with the main panel over its left edge and the bar of buttons along the bottom.
2. Press the pause button at the bottom left, or the space bar, while you build: nothing moves until you press it again. The game unpaused advances by one step each time the window's timer fires, nominally every 67 ms.

`ci/playable.txt` goes through the menu as above, with `run --seed <n>`: the seed is used for the game the menu starts too. `ci/playable-tank.txt` starts on the map of the freeform game with `run <map> --cash <cash> --seed <n>`: its seed draws the staff of its hires for that start (a game started from the menu draws other ones).

## Build an exhibit (base trial)

The four round buttons at the top of the left bar open the panels of the game. In order from the top they are: construction, adopt, objects and hire.

1. In the construction panel, the first tab (fences). Choose a fence and drag it along the side of a tile, from one end of the side to the other. The fence is laid on the side of the tile that the press is nearest to. Four drags close a square of 6 x 6 tiles; the last one asks for the name of the new exhibit. Enter keeps the name it proposes. The game puts the first gate on the exhibit itself.
2. The second tab holds paths: drag one from the gate of the exhibit to the path that leads from the entrance. The entrance already has a path.
3. Objects panel, first tab (structures): choose a food stand and a restroom and click the ground beside the path. The second tab holds scenery: a trash can and a bench. A tile that cannot hold the object shows it red.
4. Adopt panel, first tab: choose an animal and click inside the exhibit. The two buttons below the list choose male or female. Put in what the animal likes before it is unhappy: the construction panel has foliage and rocks on its other tabs, and the animal panel (click the animal) shows how well the exhibit suits it.
5. Pick an animal up: click it to select it and click it again; it follows the pointer; click on a free tile of the exhibit to put it down. A click on bare ground clears the selection. (Escape with something selected asks whether to leave the game: answer No.)
6. Hire panel: choose a keeper and click the ground by the gate, then a maintenance worker. The list shows staff of one sex at a time, drawn again after each hire; in the trials, which hold staff of one sex only, the list is empty whenever the other sex is drawn for the next hire, so a player may meet an empty Hire list. A hire costs the first month's wages at once (the staff of the base trial cost nothing).
7. Click the keeper: its panel opens. On its second tab press the assign button and click the exhibit; it is added to the list of the keeper. Escape ends the assign mode, a click on bare ground closes the panel.

Press pause again and let the game run. A month of 28 to 31 days at 12 s a day lasts about six minutes of play (main-loop spec 7.2). Guests arrive at the entrance, pay the admission, walk along the paths, look at the animals and leave; the keeper feeds the animals, the worker repairs the fences and empties the trash can.

## What to look at

- The money bar shows the cash. The zoo status button of the bar along the bottom (or the Z key) opens its window; its first tab is the Finances table, one column per month: admissions, admissions income, construction, animal purchases, upkeep, concessions, wages, donations and the totals.
- Click an animal to see its happiness, hunger, health and how well the exhibit suits it. The exhibit list button shows the exhibits with their recommendations.
- The status bar shows the happiness of the animals, of the guests and the rating of the zoo, which moves as the zoo grows.

## The tank (expansion trial)

The first tab of the construction panel lists the tank filter and the tank walls before the fences. Drag a wall along four sides like a fence; the closed area becomes a tank, filled with water at once when the game is paused. Adopt two manta rays on the water, hire a marine specialist and a tour guide and assign them to the tank as above, and let the game run for three months. The tank filter cannot be placed yet (issue 471).

## The log

`run --headless --script <file>` prints one line per event; `rustyzoo help` lists them. The checks of `ci/check-playable.sh` read these: guests created and gone (`guest add`, `guest leave`), a tour group (`guest follow`), purchases (`building buy`), the Finances table (`money cell`), wages (`sim wages`), the animals (`unit add`, `animal eat`, `animal status`, `animal dying`, `unit tile`, `unit depth`) and the rating (`sim rating`).

Run a script by hand with the line of its `#run` comment:

    ci/check-playable.sh target/release/rustyzoo <folder> ci/playable.txt

The scripts do not wait for the window's timer: a headless run steps the game as fast as it can with a clock of 200 ms a step (`--clock 200`, the cap of a step), so a month is about 1800 steps there (issue 502) and a run takes about seven minutes in a release build. The two runs of a script give the same state hash at each month change (`sim hash`) and the same log, byte for byte; CI compares the last frame and state hashes of each game between the three systems.

The header of a script says what the checks assert: `#months N` is the number of `sim month` lines the run must show (12 for the thirteenth month, 3 for the fourth), and each `#expect N <words>` line is a count of log lines the game must have printed for what the script builds and hires (the gate, the pick-up and put-down, the keeper and its assignment, the maintenance worker, the guide, the tank). A check fails when the game did not build it. An animal that is gone after 4000 s of the clock passes only if it posted `animal dying` first (old age); the happiness is compared with the animal's own first status line.
