#[test]
fn two_smc() {
    let mut system_0 = precord_core::System::new(precord_core::Features::SMC, []).unwrap();
    let mut system_1 = precord_core::System::new(precord_core::Features::SMC, []).unwrap();

    dbg!(system_0.cpus_temperature().unwrap());
    dbg!(system_1.cpus_temperature().unwrap());
}