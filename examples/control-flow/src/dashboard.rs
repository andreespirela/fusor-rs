use super::{User, *};
#[derive(FromInputs)]
pub(crate) struct Dashboard {
    #[input]
    user: Memo<User>,
    #[local(init = signal(0))]
    count: Signal<u32>,
}
fusor::template!("web/components/dashboard.html");
