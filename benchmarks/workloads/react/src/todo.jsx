import React, { useState, useMemo } from "react";
import { createRoot } from "react-dom/client";
function Todo() {
  const [tasks, setTasks] = useState(() =>
    Array.from({ length: 20 }, (_, id) => ({
      id,
      title: `Task ${id + 1}`,
      done: false,
    })),
  );
  const [title, setTitle] = useState(""),
    [filter, setFilter] = useState("all");
  const done = useMemo(() => tasks.filter((task) => task.done).length, [tasks]);
  const visible = tasks.filter(
    (task) => filter === "all" || task.done === (filter === "done"),
  );
  function add(event) {
    event.preventDefault();
    if (!title.trim()) return;
    setTasks((tasks) => [
      ...tasks,
      { id: Date.now(), title: title.trim(), done: false },
    ]);
    setTitle("");
  }
  return (
    <main>
      <h1>Tasks</h1>
      <form onSubmit={add}>
        <input
          aria-label="New task"
          value={title}
          onInput={(event) => setTitle(event.currentTarget.value)}
        />
        <button>Add task</button>
      </form>
      <nav>
        {["all", "active", "done"].map((value) => (
          <button key={value} onClick={() => setFilter(value)}>
            {value}
          </button>
        ))}
      </nav>
      <p className="task-count">
        {tasks.length} tasks · {done} done
      </p>
      <ul>
        {visible.map((task) => (
          <li key={task.id}>
            <label>
              <input
                type="checkbox"
                checked={task.done}
                onChange={(event) => {
                  const done = event.currentTarget.checked;
                  setTasks((tasks) =>
                    tasks.map((item) =>
                      item.id === task.id ? { ...item, done } : item,
                    ),
                  );
                }}
              />
              <span>{task.title}</span>
            </label>
            <button
              aria-label="Delete task"
              onClick={() =>
                setTasks((tasks) => tasks.filter((item) => item.id !== task.id))
              }
            >
              ×
            </button>
          </li>
        ))}
      </ul>
    </main>
  );
}
createRoot(document.getElementById("app")).render(<Todo />);
