//! Generates data/preflop/equity169.bin (exact all-in equity between the 169 classes).
fn main() {
    let t = std::time::Instant::now();
    let table = strategy::preflop::equity_table::generate();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/preflop/equity169.bin");
    table.save(&path).unwrap();
    let aa = engine::card::class_from_name("AA").unwrap();
    let kk = engine::card::class_from_name("KK").unwrap();
    let ako = engine::card::class_from_name("AKo").unwrap();
    let t72 = engine::card::class_from_name("72o").unwrap();
    let qq = engine::card::class_from_name("QQ").unwrap();
    println!("AA vs KK {:.4}  AKo vs QQ {:.4}  72o vs AA {:.4}  k(AA,AA) {:.2} k(AA,KK) {:.2} k(AKo,AKo) {:.2}",
        table.eq(aa, kk), table.eq(ako, qq), table.eq(t72, aa), table.k(aa, aa), table.k(aa, kk), table.k(ako, ako));
    println!("saved {} in {:.1}s", path.display(), t.elapsed().as_secs_f64());
}
