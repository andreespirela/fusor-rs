use leptos::{ev::SubmitEvent, prelude::*};
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Active,
    Done,
}

#[derive(Clone)]
struct Task {
    id: u32,
    title: String,
    done: ArcRwSignal<bool>,
}

impl Task {
    fn new(id: u32, title: String) -> Self {
        Self {
            id,
            title,
            done: ArcRwSignal::new(false),
        }
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[component]
fn Todo() -> impl IntoView {
    let tasks = RwSignal::new(
        (0..20)
            .map(|id| Task::new(id, format!("Task {}", id + 1)))
            .collect::<Vec<_>>(),
    );
    let title = RwSignal::new(String::new());
    let filter = RwSignal::new(Filter::All);
    let next = StoredValue::new(20);
    let done =
        Memo::new(move |_| tasks.with(|tasks| tasks.iter().filter(|task| task.done.get()).count()));
    let visible = Memo::new(move |_| {
        let filter = filter.get();
        tasks.with(|tasks| {
            tasks
                .iter()
                .filter(|task| filter == Filter::All || task.done.get() == (filter == Filter::Done))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let add = move |event: SubmitEvent| {
        event.prevent_default();
        let text = title.get_untracked().trim().to_owned();
        if text.is_empty() {
            return;
        }
        let id = next.get_value();
        next.set_value(id + 1);
        tasks.update(|tasks| tasks.push(Task::new(id, text)));
        title.set(String::new());
    };
    view! {
        <main>
            <h1>"Tasks"</h1>
            <form on:submit=add>
                <input aria-label="New task" bind:value=title/>
                <button>"Add task"</button>
            </form>
            <nav>
                <button on:click=move |_| filter.set(Filter::All)>"all"</button>
                <button on:click=move |_| filter.set(Filter::Active)>"active"</button>
                <button on:click=move |_| filter.set(Filter::Done)>"done"</button>
            </nav>
            <p class="task-count">{move || tasks.with(Vec::len)}" tasks · "{done}" done"</p>
            <ul>
                <For each=move || visible.get() key=|task| task.id let:task>
                    <li>
                        <label>
                            <input
                                type="checkbox"
                                prop:checked={
                                    let done = task.done.clone();
                                    move || done.get()
                                }
                                on:change={
                                    let done = task.done.clone();
                                    move |event| done.set(event_target_checked(&event))
                                }
                            />
                            <span>{task.title}</span>
                        </label>
                        <button
                            aria-label="Delete task"
                            on:click=move |_| tasks.update(|tasks| tasks.retain(|item| item.id != task.id))
                        >
                            "×"
                        </button>
                    </li>
                </For>
            </ul>
        </main>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(Todo);
}
