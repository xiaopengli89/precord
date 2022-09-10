#[test]
fn two_smc() {
    let _ = precord_core::System::new(precord_core::Features::SMC, []).unwrap();
    let _ = precord_core::System::new(precord_core::Features::SMC, []).unwrap();
}
