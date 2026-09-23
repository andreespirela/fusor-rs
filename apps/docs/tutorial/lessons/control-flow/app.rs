use fusor::prelude::*;

#[derive(Clone, PartialEq)]
struct User {
    name: String,
}

#[derive(Clone, PartialEq)]
enum Session {
    Guest,
    Authenticated { user: User },
}

struct App {
    show_help: Signal<bool>,
    session: Signal<Session>,
}

impl App {
    fn new() -> Self {
        Self {
            show_help: signal(false),
            session: signal(Session::Guest),
        }
    }

    fn sign_in(&self, name: &str) {
        self.session.set(Session::Authenticated {
            user: User { name: name.into() },
        });
    }
}

#[derive(FromInputs)]
struct Dashboard {
    #[input]
    user: Memo<User>,
    #[local(init = signal(0))]
    clicks: Signal<u32>,
}

fusor::template!("web/index.html");
fusor::template!("web/components/dashboard.html");
