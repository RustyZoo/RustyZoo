//! The ids and names of the game screen's layouts, buttons, strings and pictures that the app
//! code refers to.

/// The string that starts the default name of a guest.
pub(super) const GUEST_NAME: i32 = 9507;
/// The layout of the question boxes with a message, the main panel and the options panel, and the
/// buttons of the options panel that leave the game.
pub(super) const QUESTION: i32 = 45000;
/// The picture a question shows on its icon, and the exclamation picture of a research completion
/// (research 5.4.1).
pub(super) const QUESTION_PICTURE: &str = "ui/sharedui/question/question";
pub(super) const EXCLAIM_PICTURE: &str = "ui/sharedui/exclaim/exclaim";
pub(super) const MAIN_PANEL: i32 = 2;
/// The cash on the status bar, and its bars of animal happiness, guest happiness and zoo rating.
pub(super) const STATUS_MONEY: i32 = 1016;
pub(super) const STATUS_ANIMALS: i32 = 1011;
pub(super) const STATUS_GUESTS: i32 = 1013;
pub(super) const STATUS_ZOO: i32 = 1015;
/// The date on the status bar (its tooltip is the string of the same number), and the strings of
/// the patterns of the date and of the weekday in the tooltip.
pub(super) const STATUS_DATE: i32 = 1030;
pub(super) const DATE_PATTERN: i32 = 22012;
pub(super) const DAY_PATTERN: i32 = 22013;
pub(super) const OPTIONS_PANEL: i32 = 5;
pub(super) const MAIN_MENU_BUTTON: i32 = 1503;
pub(super) const EXIT_BUTTON: i32 = 1504;
/// The questions: save before starting a new zoo, before exiting.
pub(super) const ASK_LEAVE: i32 = 153;
pub(super) const ASK_EXIT: i32 = 151;
pub(super) const ASK_LOAD: i32 = 152;
/// The Load Game and Save Game buttons of the options panel.
pub(super) const LOAD_BUTTON: i32 = 1501;
pub(super) const SAVE_BUTTON: i32 = 1502;
