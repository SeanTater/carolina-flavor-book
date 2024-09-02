use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use handlebars::Handlebars;
use maplit::hashmap;
use recipes::{
    database::Database,
    errors::{WebError, WebResult},
    models::Recipe,
};
use serde::{Deserialize, Serialize};

lazy_static::lazy_static! {
    static ref TEMPLATES: Handlebars<'static> = handlebars();
}

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let res = recipes::search::model::EmbeddingModel::default()
        .run(&["Embed this sentence".into(), "Or this one".into()])
        .unwrap();
    println!("{:?}", res);

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `GET /recipe/:recipe_id` goes to `get_recipe`
        .route("/recipe/:recipe_id", get(get_recipe))
        .nest(
            "/static",
            axum_static::static_router("./src/static").with_state(()),
        )
        .with_state(Database::connect_default().await.unwrap());

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn handlebars() -> Handlebars<'static> {
    let mut reg = Handlebars::new();
    reg.register_template_file("index", "./src/templates/index.hbs")
        .unwrap();
    reg.register_template_file("recipe", "./src/templates/recipe.hbs")
        .unwrap();

    // Register partials
    reg.register_partial("header", include_str!("../templates/header.hbs"))
        .unwrap();
    reg.register_partial("footer", include_str!("../templates/footer.hbs"))
        .unwrap();

    reg
}

// basic handler that responds with a static string
async fn root(State(db): State<Database>) -> WebResult<Html<String>> {
    Ok(Html(TEMPLATES.render(
        "index",
        &hashmap! {"recipes" => Recipe::list_all(&db)?},
    )?))
}

async fn get_recipe(
    State(db): State<Database>,
    Path(recipe_id): Path<i64>,
) -> WebResult<Html<String>> {
    let recipe = Recipe::get_full_recipe(&db, recipe_id)?.ok_or(WebError::NotFound)?;
    Ok(Html(TEMPLATES.render("recipe", &recipe).unwrap()))
}
