use std::sync::Arc;

use axum::{
    extract::State,
    routing::{delete, get, post, put},
    Json, Router,
};
use maud::{html, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

// === App State ===
#[derive(Debug, Clone)]
struct AppStateContainer {
    state: Arc<RwLock<AppState>>,
}
impl AppStateContainer {
    fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AppState::new())),
        }
    }

    // borrow immutable state
    async fn read(&self) -> RwLockReadGuard<'_, AppState> {
        self.state.read().await
    }
    // borrow mutable state
    async fn write(&mut self) -> RwLockWriteGuard<'_, AppState> {
        self.state.write().await
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AppState {
    todos: Vec<Todo>,
}
impl AppState {
    fn new() -> Self {
        Self { todos: vec![] }
    }
    fn next_id(&self) -> u64 {
        self.todos.len() as u64 + 1
    }
    fn add(&mut self, title: String) {
        self.todos.push(Todo {
            id: self.next_id(),
            title,
            completed: false,
        });
    }
    fn toggle(&mut self, id: u64) {
        if let Some(todo) = self.todos.iter_mut().find(|todo| todo.id == id) {
            todo.completed = !todo.completed;
        }
    }
    fn remove(&mut self, id: u64) {
        self.todos.retain(|todo| todo.id != id);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Todo {
    id: u64,
    title: String,
    completed: bool,
}

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/todos", get(todos))
        .route("/create_todo", put(create_todo))
        .route("/toggle_todo", post(toggle_todo))
        .route("/remove_todo", delete(remove_todo))
        .with_state(AppStateContainer::new());

    // run our app with hyper, listening globally on port 3000
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root(state: State<AppStateContainer>) -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Magical Axum + Maud + Htmx To-Do" }
                script src="https://unpkg.com/htmx.org@1.9.10" {}
                script src="https://unpkg.com/htmx.org/dist/ext/json-enc.js" {}
                script src="https://cdn.tailwindcss.com" {}
            }
            body {
                h1 { "Magical Axum + Maud + Htmx To-Do" }
                (new_todo_html())
                div id="todos" {
                    (todos(state).await)
                }
            }
        }
    }
}

// === Components ===
// a single line item in the todo list
// without an input box, this is just a label
// and controls to toggle and remove the todo
fn todo_html(todo: &Todo) -> Markup {
    html! {
            li {
                label {
                    @if todo.completed {
                        input type="checkbox" checked hx-post="/toggle_todo" hx-target="#todos" hx-vals=(serde_json::json!({ "id": todo.id }))
                            hx-ext="json-enc" hx-swap="outerHTML";
                    } @else {
                        input type="checkbox" hx-post="/toggle_todo" hx-target="#todos" hx-vals=(serde_json::json!({ "id": todo.id }))
                            hx-ext="json-enc";
                    }
                    (todo.title)

                    button hx-delete="/remove_todo" hx-target="closest li" hx-vals=(serde_json::json!({ "id": todo.id })) hx-ext="json-enc" { "Remove" }
            }
        }
    }
}
// an input box to create a new todo
fn new_todo_html() -> Markup {
    html! {
        form hx-put="/create_todo" hx-target="#todos ul" hx-swap="beforeend" hx-ext="json-enc" "hx-on::after-request"="this.reset()" {
            input type="text" name="title" placeholder="New Todo" required;
            button type="submit" { "Add" }
        }
    }
}
fn todos_html(todos: &[Todo]) -> Markup {
    html! {
        ul {
            @for todo in todos {
                (todo_html(&todo))
            }
        }
    }
}

// === Routes ===
async fn todos(State(state): State<AppStateContainer>) -> Markup {
    let state = state.read().await;
    let todos = &state.todos;
    todos_html(todos)
}

#[derive(Deserialize)]
struct CreateTodo {
    title: String,
}
async fn create_todo(
    State(mut app_state): State<AppStateContainer>,
    Json(CreateTodo { title }): Json<CreateTodo>,
) -> Markup {
    let mut app_state = app_state.write().await;
    app_state.add(title);
    let created_todo = app_state.todos.last().unwrap();
    todo_html(&created_todo)
}

#[derive(Deserialize)]
struct ToggleTodo {
    id: u64,
}
async fn toggle_todo(
    State(mut app_state): State<AppStateContainer>,
    Json(ToggleTodo { id }): Json<ToggleTodo>,
) -> Markup {
    let mut app_state = app_state.write().await;
    app_state.toggle(id);
    todos_html(&app_state.todos)
}

#[derive(Deserialize)]
struct RemoveTodo {
    id: u64,
}
async fn remove_todo(
    State(mut app_state): State<AppStateContainer>,
    Json(RemoveTodo { id }): Json<RemoveTodo>,
) -> Markup {
    let mut app_state = app_state.write().await;
    app_state.remove(id);
    html!()
}
