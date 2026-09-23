#![cfg(feature = "dom")]

use fusor::dom::{Component, JsValue, Scope, TemplateComponent};
use fusor::{FromInputs, Owner, Signal, signal};
use std::cell::Cell;

// Construction-only tests; browser integration tests exercise real templates.
macro_rules! template_contract {
    ($($ty:ty),* $(,)?) => {$(
        impl Component for $ty {
            fn mount(self) -> Result<Scope, JsValue> { unreachable!("construction-only test") }
        }
        impl TemplateComponent for $ty {}
    )*};
}

mod child {
    use super::*;
    #[derive(FromInputs)]
    pub struct Counter {
        #[input]
        pub(super) count: Signal<i32>,
        #[local(init = signal(0))]
        pub(super) clicks: Signal<i32>,
    }
    template_contract!(Counter);
}

#[test]
fn parent_constructs_inputs_and_each_instance_gets_local_state() {
    let owner = Owner::new();
    let count = signal(0);
    let first = child::Counter::from_inputs(
        child::CounterInputs {
            count: count.clone(),
        },
        owner.handle(),
    )
    .unwrap();
    let second = child::Counter::from_inputs(
        child::CounterInputs {
            count: count.clone(),
        },
        owner.handle(),
    )
    .unwrap();
    first.count.set(7);
    first.clicks.set(2);
    assert_eq!(second.count.get(), 7);
    assert_eq!(second.clicks.get(), 0);
}

thread_local! { static CALLS: Cell<u32> = const { Cell::new(0) }; }
fn next() -> u32 {
    CALLS.with(|calls| {
        let next = calls.get() + 1;
        calls.set(next);
        next
    })
}
#[derive(FromInputs)]
struct Local {
    #[local(init = next())]
    first: u32,
    #[local(init = Self::initial())]
    second: u32,
}
impl Local {
    fn initial() -> u32 {
        next()
    }
}
#[derive(FromInputs)]
struct Empty;
#[derive(FromInputs)]
struct Braces {}
#[derive(FromInputs)]
struct Recursive {
    #[input]
    nested: Option<Box<Self>>,
    #[input]
    r#type: &'static str,
    #[input]
    bytes: [u8; Self::SIZE],
}
impl Recursive {
    const SIZE: usize = 2;
}
template_contract!(Local, Empty, Braces, Recursive);

#[test]
fn local_expressions_run_once_in_declaration_order() {
    CALLS.with(|calls| calls.set(0));
    let value = Local::from_inputs(LocalInputs {}, Owner::new().handle()).unwrap();
    assert_eq!((value.first, value.second), (1, 2));
    let value = Local::from_inputs(LocalInputs, Owner::new().handle()).unwrap();
    assert_eq!((value.first, value.second), (3, 4));
    Empty::from_inputs(EmptyInputs {}, Owner::new().handle()).unwrap();
    Braces::from_inputs(BracesInputs {}, Owner::new().handle()).unwrap();
}

#[test]
fn input_types_preserve_self_and_raw_identifiers() {
    let leaf = Recursive::from_inputs(
        RecursiveInputs {
            nested: None,
            r#type: "leaf",
            bytes: [1, 2],
        },
        Owner::new().handle(),
    )
    .unwrap();
    let parent = Recursive::from_inputs(
        RecursiveInputs {
            nested: Some(Box::new(leaf)),
            r#type: "parent",
            bytes: [3, 4],
        },
        Owner::new().handle(),
    )
    .unwrap();
    assert_eq!(parent.bytes, [3, 4]);
    assert_eq!(parent.nested.unwrap().r#type, "leaf");
}

mod renamed {
    use fusor as rf;
    #[derive(rf::FromInputs)]
    #[from_inputs(crate = rf)]
    pub(super) struct Renamed {
        #[input]
        pub(super) value: String,
    }
    template_contract!(Renamed);
    use super::{Component, JsValue, Scope, TemplateComponent};
}

#[test]
fn explicit_runtime_path_works_with_reexports() {
    let value = renamed::Renamed::from_inputs(
        renamed::RenamedInputs {
            value: "hello".into(),
        },
        Owner::new().handle(),
    )
    .unwrap();
    assert_eq!(value.value, "hello");
}

#[derive(FromInputs)]
struct Conditional {
    #[cfg(not(feature = "dom"))]
    absent: MissingType,
    #[cfg_attr(feature = "dom", input)]
    value: bool,
    #[cfg(feature = "dom")]
    #[local(init = 3)]
    local: u32,
}
template_contract!(Conditional);

#[test]
fn conditional_fields_follow_rust_configuration() {
    let value =
        Conditional::from_inputs(ConditionalInputs { value: true }, Owner::new().handle()).unwrap();
    assert!(value.value);
    assert_eq!(value.local, 3);
}
