use blueoath_server::router::{GameMethod, MethodFamily};

#[test]
fn classifies_registered_method_families_without_scattered_prefix_checks() {
    let method = GameMethod::parse("battle.StartBattle");
    assert_eq!(method.family(), MethodFamily::Battle);
    assert!(method.is_family(MethodFamily::Battle));
    assert!(method.is_known());
}

#[test]
fn preserves_unknown_methods_for_explicit_boundary_errors() {
    let method = GameMethod::parse("not.registered");
    assert_eq!(method.family(), MethodFamily::Unknown);
    assert!(!method.is_known());
    assert_eq!(method.name(), "not.registered");
}
