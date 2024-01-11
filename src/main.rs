use std::sync::Arc;

use axum::{
    extract::State,
    routing::{delete, get, post, put},
    Json, Router,
};
use maud::{html, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};
use sled::Db;
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

#[derive(Debug)]
struct AppState {
    handle: Db,
}
impl AppState {
    fn new() -> Self {
        Self {
            handle: sled::open("todos").unwrap(),
        }
    }
    fn next_id(&self) -> u64 {
        self.handle.generate_id().unwrap()
    }
    fn add(&mut self, title: String) {
        let id = self.next_id();
        let id_bytes = id.to_be_bytes();
        let todo = Todo {
            id,
            title,
            completed: false,
        };
        let todo_json = serde_json::to_string(&todo).unwrap();
        let todo_bytes = todo_json.as_bytes();
        self.handle.insert(id_bytes, todo_bytes).unwrap();
    }
    fn iter(&self) -> impl Iterator<Item = Todo> {
        self.handle
            .iter()
            .map(|res| res.unwrap())
            .map(|(_, v)| v)
            .map(|v| serde_json::from_slice(&v).unwrap())
    }
    fn toggle(&mut self, id: u64) {
        let id_bytes = id.to_be_bytes();
        let todo_bytes = self.handle.get(id_bytes).unwrap().unwrap();
        let mut todo: Todo = serde_json::from_slice(&todo_bytes).unwrap();
        todo.completed = !todo.completed;
        let todo_json = serde_json::to_string(&todo).unwrap();
        let todo_bytes = todo_json.as_bytes();
        self.handle.insert(id_bytes, todo_bytes).unwrap();
    }
    fn remove(&mut self, id: u64) {
        let id_bytes = id.to_be_bytes();
        self.handle.remove(id_bytes).unwrap();
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
    println!("Listening on http://{}", listener.local_addr().unwrap());
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
            body class="bg-gray-100 font-sans leading-normal tracking-normal" {
                div class="container mx-auto p-8" {
                    h1 class="text-4xl text-center text-gray-700 mb-6" { "Magical Axum + Maud + Htmx To-Do" }
                    (new_todo_html())
                    div id="todos" class="mt-6" {
                        (todos(state).await)
                    }
                }
            }
        }
    }
}

// === Components ===
// a single line item in the todo list
fn todo_html(todo: &Todo) -> Markup {
    html! {
        li class="flex items-center bg-white rounded-lg shadow-lg my-2 py-2 px-4" {
            label class="flex-grow" {
                @if todo.completed {
                    input type="checkbox" checked class="mr-2" hx-post="/toggle_todo" hx-target="closest li" hx-vals=(serde_json::json!({ "id": todo.id }))
                        hx-ext="json-enc" hx-swap="outerHTML";
                } @else {
                    input type="checkbox" class="mr-2" hx-post="/toggle_todo" hx-target="closest li" hx-vals=(serde_json::json!({ "id": todo.id }))
                        hx-ext="json-enc" hx-swap="outerHTML";
                }
                span class={@if todo.completed { "line-through" } @else { "" }} { (todo.title) }
            }
            button class="bg-red-500 hover:bg-red-700 text-white font-bold py-1 px-2 rounded" hx-delete="/remove_todo" hx-target="closest li" hx-swap="outerHTML" hx-vals=(serde_json::json!({ "id": todo.id })) hx-ext="json-enc" { "Remove" }
        }
    }
}

// an input box to create a new todo
fn new_todo_html() -> Markup {
    html! {
        form class="flex justify-between items-center" hx-put="/create_todo" hx-target="#todos ul" hx-swap="beforeend" hx-ext="json-enc" "hx-on::after-request"="this.reset()" {
            input class="w-full rounded p-2 mr-4" type="text" name="title" placeholder="New Todo" required;
            button class="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded" type="submit" { "Add" }
        }
    }
}

fn todos_html(todos: &[Todo]) -> Markup {
    html! {
        ul class="list-none p-0" {
            @for todo in todos {
                (todo_html(&todo))
            }
        }
    }
}

// === Routes ===
async fn todos(State(state): State<AppStateContainer>) -> Markup {
    let state = state.read().await;
    let todos = &state.iter().collect::<Vec<_>>();
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
    let todos = &app_state.iter().collect::<Vec<_>>();
    let created_todo = todos.last().unwrap();
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
    let todos = &app_state.iter().collect::<Vec<_>>();
    let toggled_todo = todos.iter().find(|todo| todo.id == id).unwrap();
    todo_html(&toggled_todo)
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
