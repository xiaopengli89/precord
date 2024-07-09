#[test]
fn two_smc() {
    let _ = precord_core::System::new(precord_core::Features::SMC, []);
    let _ = precord_core::System::new(precord_core::Features::SMC, []);
}
