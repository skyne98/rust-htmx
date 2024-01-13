pub mod db;
pub mod error;
pub mod models;
pub mod repository;

use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    routing::{delete, get, post, put},
    Form, Json, Router,
};
use db::driver::Db;
use error::AppError;
use maud::{html, Markup, DOCTYPE};
use models::Todo;
use serde::Deserialize;
use tokio::{
    net::TcpListener,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

// === App State ===
#[derive(Debug, Clone)]
struct AppState {
    state: Arc<RwLock<Db>>,
}
impl AppState {
    fn new() -> Result<Self> {
        Ok(Self {
            state: Arc::new(RwLock::new(Db::new()?)),
        })
    }

    // borrow immutable state
    async fn read(&self) -> RwLockReadGuard<'_, Db> {
        self.state.read().await
    }
    // borrow mutable state
    async fn write(&mut self) -> RwLockWriteGuard<'_, Db> {
        self.state.write().await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let state = AppState::new()?;
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/todos", get(todos))
        .route("/create_todo", put(create_todo))
        .route("/toggle_todo", post(toggle_todo))
        .route("/remove_todo", delete(remove_todo))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await?;
    Ok(())
}

// basic handler that responds with a static string
async fn root(state: State<AppState>) -> Result<Markup, AppError> {
    Ok(html! {
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
                        (todos(state).await?)
                    }
                }
            }
        }
    })
}

// === Components ===
// a single line item in the todo list
fn todo_html(todo: &Todo) -> Markup {
    html! {
        li class="flex items-center bg-white rounded-lg shadow-lg my-2 py-2 px-4" {
            label class="flex-grow" {
                @if todo.completed {
                    input type="checkbox" checked class="mr-2" hx-post="/toggle_todo" hx-target="closest li" hx-vals=(serde_json::json!({ "id": todo.id }))
                        hx-swap="outerHTML";
                } @else {
                    input type="checkbox" class="mr-2" hx-post="/toggle_todo" hx-target="closest li" hx-vals=(serde_json::json!({ "id": todo.id }))
                        hx-swap="outerHTML";
                }
                span class={@if todo.completed { "line-through" } @else { "" }} { (todo.title) }
            }
            button class="bg-red-500 hover:bg-red-700 text-white font-bold py-1 px-2 rounded" hx-delete="/remove_todo" hx-target="closest li" hx-swap="outerHTML" hx-vals=(serde_json::json!({ "id": todo.id })) { "Remove" }
        }
    }
}

// an input box to create a new todo
fn new_todo_html() -> Markup {
    html! {
        form class="flex justify-between items-center" hx-put="/create_todo" hx-target="#todos ul" hx-swap="beforeend" "hx-on::after-request"="this.reset()" {
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
async fn todos(State(state): State<AppState>) -> Result<Markup, AppError> {
    let state = state.read().await;
    let mut todos = state.iter_prefix::<Todo>("todo")?;
    let mut todos_vec = Vec::new();
    for todo_result in &mut todos {
        if let Ok((_, todo)) = todo_result {
            todos_vec.push(todo);
        } else {
            return Err(anyhow::anyhow!("Error getting todos").into());
        }
    }
    Ok(todos_html(&todos_vec))
}

#[derive(Deserialize)]
struct CreateTodo {
    title: String,
}
async fn create_todo(
    State(mut app_state): State<AppState>,
    Form(CreateTodo { title }): Form<CreateTodo>,
) -> Result<Markup, AppError> {
    let app_state = app_state.write().await;
    let id = app_state.next_id()?;
    let todo = Todo::new(id, title);
    let key = format!("todo:{}", id);
    app_state.insert(&key, &todo)?;
    Ok(todo_html(&todo))
}

#[derive(Deserialize)]
struct ToggleTodo {
    id: u64,
}
async fn toggle_todo(
    State(mut app_state): State<AppState>,
    Form(ToggleTodo { id }): Form<ToggleTodo>,
) -> Result<Markup, AppError> {
    let app_state = app_state.write().await;
    let key = format!("todo:{}", id);
    let mut todo = app_state.get::<Todo, _>(&key)?;
    if let Some(ref mut todo) = todo {
        todo.completed = !todo.completed;
        app_state.insert(&key, &todo)?;
    }
    let todo = todo.unwrap();
    Ok(todo_html(&todo))
}

#[derive(Deserialize)]
struct RemoveTodo {
    id: u64,
}
async fn remove_todo(
    State(mut app_state): State<AppState>,
    Form(RemoveTodo { id }): Form<RemoveTodo>,
) -> Result<Markup, AppError> {
    let app_state = app_state.write().await;
    let key = format!("todo:{}", id);
    app_state.remove(&key)?;
    Ok(html! {})
}
