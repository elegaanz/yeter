use yeter::Database;
use std::hash::Hash;

#[yeter::query]
fn len_ref<'a>(_db: &Database, str: &'a str) -> usize {
    str.len()
}

#[test]
fn lifetimes() {
    let mut db = Database::new();
    db.register_impl::<len_ref>();
    assert_eq!(*len_ref(&db, "hello"), 5);
}

#[yeter::query]
fn get_first<T: Copy + 'static, A: AsRef<[T]> + Hash>(db: &Database, array: A) -> Option<T> {
    let array: &[T] = array.as_ref();
    array.first().copied()
}

#[test]
fn types() {
    let mut db = Database::new();
    db.register_impl::<get_first::<_, [u8; 3]>>();
    db.register_impl::<get_first::<_, Vec<u16>>>();
    assert_eq!(*get_first(&db, [1u8, 2, 3]), Some(1));
    assert_eq!(*get_first(&db, vec![4u16, 5, 6]), Some(4));
}

#[yeter::query]
fn create_zeroed<const N: usize>(_db: &Database) -> [u8; N] {
    [0; N]
}

#[test]
fn consts() {
    let mut db = Database::new();
    db.register_impl::<create_zeroed::<4>>();
    assert_eq!(create_zeroed::<4>(&db).as_slice(), [0, 0, 0, 0]);
}
