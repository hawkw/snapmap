use super::*;

#[test]
fn fuzz_insert() {
    loom::model(|| {
        let map = SnapMap::new();
        let mut w1 = map.writer();
        let mut w2 = map.writer();
        let j1 = loom::thread::spawn(move || {
            w1.insert(1, "world");
            w1.insert(2, "earth");
        });
        let j2 = loom::thread::spawn(move || {
            w2.insert(3, "san francisco");
            w2.insert(4, "oakland");
        });
        let snap = map.snapshot();
        for (_, v) in &snap {
            println!("hello {}", v);
        }
        j1.join().unwrap();
        j2.join().unwrap();
    });
}
