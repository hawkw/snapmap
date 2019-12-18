use super::*;

#[test]
fn fuzz_insert() {
    loom::model(|| {
        println!("\n ---- ITERATION ---- \n");
        let map = SnapMap::new();
        let mut w1 = map.writer();
        let mut w2 = map.writer();
        let j1 = loom::thread::spawn(move || {
            w1.insert(1, "world");

            println!("t1 insert 1");
            w1.insert(2, "earth");

            println!("t1 insert 2");
        });
        let j2 = loom::thread::spawn(move || {
            println!("t2 insert 1");
            w2.insert(3, "san francisco");

            println!("t2 insert 1");
            w2.insert(4, "oakland");
        });
        {
            let snap = map.snapshot();
            println!("snap");
            for (_, v) in &snap {
                println!("hello {}", v);
            }
            drop(snap)
        }
        j1.join().unwrap();
        j2.join().unwrap();
    });
}
