mod string {
    #[yeter::query]
    pub fn len(_db: &yeter::Database, input: String) -> usize {
        input.len()
    }
}

fn main() {
    let db = yeter::Database::new();
    let len1 = string::len(&db, "hello".into());
    let len2 = string::len(&db, "hello".into());
    let len3 = string::len(&db, "world".into());
    assert_eq!(len1, len2);
    assert_eq!(len1, len3);
}