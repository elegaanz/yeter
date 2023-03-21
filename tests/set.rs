use yeter::Database;

#[yeter::query]
fn setable_query(db: &Database, input: String) -> String;

#[yeter::query]
fn id(db: &Database, input: String) -> String {
    setable_query(db, input).to_string()
}

fn main() {
    let mut db = Database::new();

    // TODO(autoreg): this is not working, but once auto registering it won't be needed anymore
    db.register_impl::<setable_query>();

    db.register_impl::<id>();
}