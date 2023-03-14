yeter::queries! {
    string {
        len : String : usize
    }
}

fn main() {
    let mut db = yeter::Database::new();
    db.register::<_, string::len>(|db, name| {
        if name.len() == 0 {
            db.do_effect("An empty string was passed");
        }
        dbg!(name.len())
    });
    let len1 = string::len(&db, "".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [1]: {}", msg);
    }
    let len2 = string::len(&db, "".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [2]: {}", msg);
    }
    let len3 = string::len(&db, "aaaa".into());
    for msg in db.effect::<&'static str>() {
        println!("EFFECT [3]: {}", msg);
    }
    assert_eq!(len1, len2);
    assert_eq!(*len3, 4);
}