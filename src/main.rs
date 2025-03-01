#[macro_use]
extern crate rocket;

use rocket::response::content::{RawCss, RawHtml, RawJavaScript, RawText};
use rocket::serde::{Deserialize, Serialize};
use rocket::State;
use sqlx::types::chrono::{NaiveDate, NaiveDateTime};
use sqlx::{PgPool, Row};
use std::env;
use rocket::response::stream::TextStream;

const INDEX_HTML: &'static str = include_str!("../static/index.html");
const DYGRAPH_JS: &'static str = include_str!("../static/dygraph.min.js");
const DYGRAPH_CSS: &'static str = include_str!("../static/dygraph.css");
const ALLOWED_QUERY_COL: [&'static str; 13] = [
    "charge",
    "tension_bat",
    "tension_pv1",
    "tension_pv2",
    "temperature",
    "courant_pv1",
    "courant_pv2",
    "entree_energie_24h",
    "sortie_energie_24h",
    "courant_entree_appareil",
    "courant_charge_total",
    "courant_consommateur",
    "courant_decharge_total",
];

#[get("/")]
fn index() -> RawHtml<&'static str> {
    RawHtml(INDEX_HTML)
}

#[get("/dygraph.min.js")]
fn dygraph_js() -> RawJavaScript<&'static str> {
    RawJavaScript(DYGRAPH_JS)
}

#[get("/dygraph.css")]
fn dygraph_css() -> RawCss<&'static str> {
    RawCss(DYGRAPH_CSS)
}

#[derive(Serialize, Deserialize, FromForm)]
struct QueryParams {
    #[field(name = "data", default = "charge")]
    data: String,
    #[field(name = "modulo", default = 10i32)]
    modulo: i32,
    #[field(name = "startdate", default = "2000-01-01")]
    startdate: String,
    #[field(name = "stopdate", default = "3000-01-01")]
    stopdate: String,
}

#[get("/data?<params..>")]
async fn data(
    pool: &State<PgPool>,
    params: QueryParams,
) -> Result<RawText<String>, rocket::http::Status> {
    let data_column = params.data;
    if !ALLOWED_QUERY_COL.contains(&data_column.as_str()) {
        // avoid SQL injection
        return Err(rocket::http::Status::BadRequest);
    }

    let query_str = format!("SELECT servdate, {} FROM battery WHERE id % $1 = 0 AND erreur_crc16 = FALSE AND servdate >= $2 AND servdate <= $3 ORDER BY id ASC;", data_column);

    let start_date = NaiveDate::parse_from_str(params.startdate.as_str(), "%Y-%m-%d")
        .map_err(|_| rocket::http::Status::BadRequest)?;

    let stop_date = NaiveDate::parse_from_str(params.stopdate.as_str(), "%Y-%m-%d")
        .map_err(|_| rocket::http::Status::BadRequest)?;

    let rows = sqlx::query(&query_str)
        .bind(params.modulo)
        .bind(start_date)
        .bind(stop_date)
        .fetch_all(pool.inner())
        .await
        .map_err(|_| rocket::http::Status::InternalServerError)?;

    let mut csv_output = String::new();

    for row in rows {
        let servdate: NaiveDateTime = row
            .try_get("servdate")
            .map_err(|_| rocket::http::Status::InternalServerError)?;
        let data_value: Option<f32> = row
            .try_get(data_column.as_str())
            .map_err(|_| rocket::http::Status::InternalServerError)?;
        csv_output.push_str(&format!(
            "{},{}\n",
            servdate.format("%Y-%m-%d %H:%M:%S"),
            data_value.unwrap_or(f32::NAN)
        ));
        /*yield format!(
               "{},{}\n",
               servdate.format("%Y-%m-%d %H:%M:%S"),
               data_value.unwrap_or(f32::NAN)
           );*/
    }

    Ok(RawText(csv_output))
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    dotenv::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let _ = rocket::build()
        .manage(pool)
        .mount("/", routes![index, dygraph_js, dygraph_css, data])
        .launch()
        .await?;

    Ok(())
}
