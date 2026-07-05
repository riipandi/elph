use elph_tui::{DiffContainer, LineComponent, Loader, Markdown, Text};

#[test]
fn container_stacks_text_markdown_and_loader() {
    let mut container = DiffContainer::new();
    container.add_child(Box::new(Text::new("Status line")));
    container.add_child(Box::new(Markdown::new("## Answer\n\nDone.")));
    let mut loader = Loader::new("Thinking");
    loader.start();
    container.add_child(Box::new(loader));

    let lines = container.render(50);
    assert!(lines.len() >= 3);
    let joined = lines.join("\n");
    assert!(joined.contains("Status"));
    assert!(joined.contains("Answer"));
    assert!(joined.contains("Thinking"));
}
