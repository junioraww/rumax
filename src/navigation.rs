use rand::distributions::{Distribution, WeightedIndex};
use rand::Rng;
use std::collections::VecDeque;
use std::time::Duration;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Screen {
    Background = 1,
    Contacts = 100,
    Chats = 150,
    Search = 151,
    Calls = 300,
    Chat = 350,
    Settings = 450,
    MiniApp = 500,
}

impl Screen {
    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn name(self) -> &'static str {
        match self {
            Screen::Background => "BACKGROUND",
            Screen::Contacts => "CONTACTS",
            Screen::Chats => "CHATS",
            Screen::Search => "SEARCH",
            Screen::Calls => "CALLS",
            Screen::Chat => "CHAT",
            Screen::Settings => "SETTINGS",
            Screen::MiniApp => "MINIAPP",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RouteProfile {
    pub steps: u32,
    pub min_pause: f64,
    pub max_pause: f64,
    pub long_pause_chance: f64,
    pub min_long_pause: f64,
    pub max_long_pause: f64,
    pub back_chance: f64,
}

impl RouteProfile {
    pub fn get_pause_duration(&self) -> Duration {
        let mut rng = rand::thread_rng();
        let secs = if rng.gen_bool(self.long_pause_chance) {
            rng.gen_range(self.min_long_pause..=self.max_long_pause)
        } else {
            rng.gen_range(self.min_pause..=self.max_pause)
        };
        Duration::from_secs_f64(secs)
    }
}

pub fn get_random_profile() -> RouteProfile {
    let mut rng = rand::thread_rng();
    match rng.gen_range(0..3) {
        0 => RouteProfile { // "quick"
            steps: 2, min_pause: 35.0, max_pause: 95.0,
            long_pause_chance: 0.05, min_long_pause: 180.0, max_long_pause: 420.0, back_chance: 0.30,
        },
        1 => RouteProfile { // "browse"
            steps: 4, min_pause: 70.0, max_pause: 210.0,
            long_pause_chance: 0.12, min_long_pause: 240.0, max_long_pause: 720.0, back_chance: 0.22,
        },
        _ => RouteProfile { // "read"
            steps: 3, min_pause: 140.0, max_pause: 360.0,
            long_pause_chance: 0.25, min_long_pause: 420.0, max_long_pause: 1200.0, back_chance: 0.18,
        },
    }
}

fn get_transitions(screen: Screen) -> &'static [(Screen, u32)] {
    match screen {
        Screen::Background => &[(Screen::Chats, 10), (Screen::Settings, 1)],
        Screen::Chats => &[
            (Screen::Chat, 7), (Screen::Contacts, 2), (Screen::Search, 2),
            (Screen::Calls, 1), (Screen::Settings, 1), (Screen::Chats, 2)
        ],
        Screen::Chat => &[
            (Screen::Chats, 8), (Screen::Chat, 2), (Screen::Settings, 1)
        ],
        Screen::Contacts => &[
            (Screen::Chats, 6), (Screen::Chat, 2), (Screen::Search, 1)
        ],
        Screen::Search => &[
            (Screen::Chats, 5), (Screen::Chat, 3), (Screen::Contacts, 1)
        ],
        Screen::Calls => &[
            (Screen::Chats, 5), (Screen::Contacts, 2), (Screen::Settings, 2)
        ],
        Screen::Settings => &[
            (Screen::Chats, 7), (Screen::Contacts, 2), (Screen::Calls, 2), (Screen::MiniApp, 1)
        ],
        Screen::MiniApp => &[
            (Screen::Settings, 3), (Screen::Chats, 6)
        ],
    }
}

pub struct NavigationPlanner {
    pub current_screen: Screen,
    history: VecDeque<Screen>,
}

impl NavigationPlanner {
    pub fn new() -> Self {
        Self {
            current_screen: Screen::Background,
            history: VecDeque::with_capacity(4),
        }
    }

    pub fn reset_to_background(&mut self) {
        self.current_screen = Screen::Background;
        self.history.clear();
    }

    pub fn next_screen(&mut self, profile: &RouteProfile) -> Screen {
        let mut rng = rand::thread_rng();

        if !self.history.is_empty() && rng.gen_bool(profile.back_chance) {
            self.current_screen = self.history.pop_back().unwrap();
            return self.current_screen;
        }

        let transitions = get_transitions(self.current_screen);
        let weights: Vec<u32> = transitions.iter().map(|&(_, w)| w).collect();
        let dist = WeightedIndex::new(&weights).expect("Неверные веса графа переходов");
        let next_screen = transitions[dist.sample(&mut rng)].0;

        if next_screen != self.current_screen {
            self.history.push_back(self.current_screen);
            if self.history.len() > 4 {
                self.history.pop_front();
            }
        }

        self.current_screen = next_screen;
        self.current_screen
    }
}
