use aimer::style::{LayoutSpacing, Spacing};
use aimer::{AimerApp, AsyncBuilder, AsyncSnapshot, Color, Container, Text, Widget};

const EXAMPLE_URL: &str = "https://example.com";

async fn fetch_example() -> Result<String, String> {
    reqwest::get(EXAMPLE_URL)
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .await
        .map_err(|error| error.to_string())
}

fn snapshot_message(snapshot: &AsyncSnapshot<String, String>) -> String {
    match snapshot {
        AsyncSnapshot::Waiting => "Loading example.com...".to_owned(),
        AsyncSnapshot::Data(body) => body.clone(),
        AsyncSnapshot::Error(error) => format!("Request failed: {error}"),
    }
}

pub fn async_builder_example() -> impl Widget {
    Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(24)))
        .color(Color::WHITE)
        .child(
            AsyncBuilder::new()
                .request_key(EXAMPLE_URL)
                .future(fetch_example)
                .child(|snapshot| Text::new(snapshot_message(snapshot)).boxed()),
        )
}

pub fn start_async_builder_example() {
    AimerApp::start(async_builder_example());
}

#[cfg(test)]
mod tests {
    use aimer::{AsyncSnapshot, Widget};

    use super::{async_builder_example, snapshot_message};

    #[test]
    fn snapshot_message_covers_loading_success_and_error() {
        assert_eq!(snapshot_message(&AsyncSnapshot::Waiting), "Loading example.com...");
        assert_eq!(
            snapshot_message(&AsyncSnapshot::Data("Example Domain".to_owned())),
            "Example Domain"
        );
        assert_eq!(
            snapshot_message(&AsyncSnapshot::Error("request failed".to_owned())),
            "Request failed: request failed"
        );
    }

    #[test]
    fn example_builds_without_starting_the_request() {
        fn assert_widget(_widget: impl Widget) {}

        assert_widget(async_builder_example());
    }
}
