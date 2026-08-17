//! ACP v2 helper coverage (content + advertised commands).

//! ACP v2 is linked into the `elph` library (`elph acp`).

#[test]
fn acp_module_is_linked() {
    let _ = std::any::type_name_of_val(&elph::platform::acp::run_agent_stdio);
}
