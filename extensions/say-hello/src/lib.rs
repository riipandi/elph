wit_bindgen::generate!({
    path: "../../crates/elph-agent/wit/extension.wit",
    world: "guest",
});

struct SayHello;

impl Guest for SayHello {
    fn list_commands() -> Vec<CommandDesc> {
        vec![CommandDesc {
            name: "say-hello".into(),
            description: "Greet someone by name".into(),
        }]
    }

    fn execute_command(name: String, args: String) -> Result<SlashResult, String> {
        if name != "say-hello" {
            return Err(format!("unknown command: {name}"));
        }
        let target = args.trim();
        let message = if target.is_empty() {
            "Hello, world!".to_string()
        } else {
            format!("Hello, {target}!")
        };
        Ok(SlashResult {
            message,
            is_error: false,
        })
    }
}

export!(SayHello);