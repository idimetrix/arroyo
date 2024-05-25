use cornucopia::{CodegenSettings, Error};
use postgres::{Client, NoTls};

fn main() -> Result<(), Error> {
    let queries_path = "queries";
    let destination = format!("{}/api-sql.rs", std::env::var("OUT_DIR").unwrap());
    let settings = CodegenSettings {
        gen_async: true,
        derive_ser: true,
        gen_sync: false,
        gen_sqlite: true,
    };

    println!("cargo:rerun-if-changed={queries_path}");
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=sqlite_migrations");

    let mut client = Client::configure()
        .dbname("arroyo")
        .host("localhost")
        .port(5432)
        .user("arroyo")
        .password("arroyo")
        .connect(NoTls)
        .unwrap_or_else(|_| {
            panic!("Could not connect to postgres: arroyo:arroyo@localhost:5432/arroyo")
        });

    let mut sqlite =
        rusqlite::Connection::open_in_memory().expect("Couldn't open sqlite memory connection");
    let migrations = refinery::load_sql_migrations("sqlite_migrations").unwrap();
    refinery::Runner::new(&migrations)
        .run(&mut sqlite)
        .expect("Failed to run migrations");

    cornucopia::generate_live_with_sqlite(
        &mut client,
        queries_path,
        Some(&destination),
        &sqlite,
        settings,
    )?;

    Ok(())
}
