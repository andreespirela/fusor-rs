import { createSignal, createMemo, For } from "solid-js";
import { createStore } from "solid-js/store";
import { render } from "solid-js/web";
function Todo() {
  const [tasks, setTasks] = createStore(
    Array.from({ length: 20 }, (_, id) => ({
      id,
      title: `Task ${id + 1}`,
      done: false,
    })),
  );
  const [title, setTitle] = createSignal(""),
    [filter, setFilter] = createSignal("all");
  const done = createMemo(() => tasks.filter((task) => task.done).length);
  const visible = createMemo(() =>
    tasks.filter(
      (task) => filter() === "all" || task.done === (filter() === "done"),
    ),
  );
  function add(event) {
    event.preventDefault();
    if (!title().trim()) return;
    setTasks(tasks.length, {
      id: Date.now(),
      title: title().trim(),
      done: false,
    });
    setTitle("");
  }
  return (
    <main>
      <h1>Tasks</h1>
      <form onSubmit={add}>
        <input
          aria-label="New task"
          value={title()}
          onInput={(event) => setTitle(event.currentTarget.value)}
        />
        <button>Add task</button>
      </form>
      <nav>
        <For each={["all", "active", "done"]}>
          {(value) => <button onClick={() => setFilter(value)}>{value}</button>}
        </For>
      </nav>
      <p class="task-count">
        {tasks.length} tasks · {done()} done
      </p>
      <ul>
        <For each={visible()}>
          {(task) => (
            <li>
              <label>
                <input
                  type="checkbox"
                  checked={task.done}
                  onChange={(event) =>
                    setTasks(
                      (item) => item.id === task.id,
                      "done",
                      event.currentTarget.checked,
                    )
                  }
                />
                <span>{task.title}</span>
              </label>
              <button
                aria-label="Delete task"
                onClick={() =>
                  setTasks(tasks.filter((item) => item.id !== task.id))
                }
              >
                ×
              </button>
            </li>
          )}
        </For>
      </ul>
    </main>
  );
}
render(Todo, document.getElementById("app"));
