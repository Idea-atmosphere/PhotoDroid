//! Privacy-preserving diagnostics for user operations. Never pass a path, error message,
//! parameters, document content or typed text to this module.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Save,
    Export,
    // This is a name from the built-in command registry, never a user-provided string.
    Filter(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Started,
    Pending,
    Succeeded,
    Unconfirmed,
    Failed,
    Cancelled,
    PickerOpened,
    PickerSelected,
}

impl Operation {
    fn label(self) -> String {
        match self {
            Self::Save => "save".into(),
            Self::Export => "export".into(),
            Self::Filter(id) => format!("filter {id}"),
        }
    }
}

impl Event {
    fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Unconfirmed => "submitted (write unconfirmed)",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::PickerOpened => "picker opened",
            Self::PickerSelected => "picker selected",
        }
    }
}

fn line(operation: Operation, event: Event) -> String {
    format!("operation: {}: {}", operation.label(), event.label())
}

/// Uses the existing logging switch and persistence; emits no line at all while logging is off.
pub(crate) fn record(operation: Operation, event: Event) {
    photocraft_doc::diag::note(|| line(operation, event));
}

/// A filter name must come from the built-in command registry, not parameters or an unknown
/// command supplied by a caller. Some export commands also run directly rather than via a picker.
pub(crate) fn command(id: &str) -> Option<Operation> {
    if matches!(id, "file.export.quickExport" | "file.export.saveForWebLegacy") {
        return Some(Operation::Export);
    }
    if !id.starts_with("filter.") {
        return None;
    }
    photocraft_engine::command_specs().iter().find(|spec| spec.id == id).map(|spec| Operation::Filter(spec.id))
}

/// A completed worker, not merely a submitted job.
pub(crate) fn job_outcome(outcome: &photocraft_engine::jobs::JobOutcome) -> Event {
    use photocraft_engine::jobs::JobOutcome;
    match outcome {
        JobOutcome::Done(_) => Event::Succeeded,
        JobOutcome::Failed(_) => Event::Failed,
        JobOutcome::Cancelled => Event::Cancelled,
    }
}

/// An accepted background job is not a successful operation until its completion event arrives.
pub(crate) fn result(result: &Result<Value, String>) -> Event {
    match result {
        Ok(v) if v.get("pending").and_then(Value::as_bool) == Some(true) => Event::Pending,
        Ok(_) => Event::Succeeded,
        Err(e) if e == crate::file_dialog::CANCELLED => Event::Cancelled,
        Err(_) => Event::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diagnostic_log_actions_are_bounded_and_do_not_include_user_data() {
        assert_eq!(command("filter.stylize.emboss"), Some(Operation::Filter("filter.stylize.emboss")));
        assert_eq!(command("filter.private text /document"), None);
        assert_eq!(command("file.export.quickExport"), Some(Operation::Export));
        assert_eq!(command("file.export.presets.save"), None);
        assert_eq!(result(&Ok(json!({"pending": true, "path": "/secret/name"}))), Event::Pending);
        assert_eq!(result(&Ok(json!({"path": "/secret/name"}))), Event::Succeeded);
        assert_eq!(result(&Err("cancelled".into())), Event::Cancelled);
        assert_eq!(result(&Err("/secret/name: private error".into())), Event::Failed);
        use photocraft_engine::jobs::JobOutcome;
        assert_eq!(job_outcome(&JobOutcome::Done(json!({"path": "/secret"}))), Event::Succeeded);
        assert_eq!(job_outcome(&JobOutcome::Failed("/secret".into())), Event::Failed);
        assert_eq!(job_outcome(&JobOutcome::Cancelled), Event::Cancelled);
        for event in [Event::Started, Event::Pending, Event::Succeeded, Event::Unconfirmed, Event::Failed, Event::Cancelled, Event::PickerOpened, Event::PickerSelected] {
            for op in [Operation::Save, Operation::Export, Operation::Filter("filter.stylize.emboss")] {
                let rendered = line(op, event);
                assert!(!rendered.contains("/secret") && !rendered.contains("private error"));
            }
        }
    }
}
