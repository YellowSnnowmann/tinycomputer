//! A person at the terminal, for the pauses only a person can answer: an
//! irreversible action to approve, a login or captcha to get past, a detail
//! the task was not given, and a payment page left to them.
//!
//! Without one, `follow` hands every such pause back and the runner ends;
//! with one, the task waits for them with its browser open and goes on.

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

use tinycomputer_bus::agent::{ContinueTaskRequest, InputField, TaskId, TaskStatus};

/// Someone who can act for a paused task.
pub trait Person: Send + Sync {
    /// Whether the task may press `target` to `action`.
    fn approve(&self, action: &str, target: &str) -> bool;
    /// Whether they did what `reason` asks in the browser (a login, a
    /// captcha, a one-time code), so the task may go on.
    fn handled(&self, reason: &str) -> bool;
    /// A value for `field`, which the task needs and was not given.
    fn input(&self, field: &InputField) -> Option<String>;
    /// The task stopped where only they go on, for `reason` (a payment
    /// page); they finish there before the browser is closed.
    fn finish(&self, reason: &str);
}

/// What `person` tells the task paused at `status`, or `None` to stop
/// following it. A `needs_input` takes `answers` first and asks the person
/// only for what they lack; a declined approval is sent too, so the task
/// learns it was declined and stops.
#[must_use]
pub fn reply(
    id: &TaskId,
    status: &TaskStatus,
    answers: &BTreeMap<String, String>,
    person: &dyn Person,
) -> Option<ContinueTaskRequest> {
    let request = |continued: ContinueTaskRequest| ContinueTaskRequest {
        id: id.clone(),
        ..continued
    };
    match status {
        TaskStatus::NeedsApproval { action, target, .. } => Some(request(ContinueTaskRequest {
            approve: Some(person.approve(action, target)),
            ..ContinueTaskRequest::default()
        })),
        TaskStatus::Checkpoint {
            reason,
            continuable: true,
            ..
        } => Some(request(ContinueTaskRequest {
            approve: Some(person.approve("go past the checkpoint", reason)),
            ..ContinueTaskRequest::default()
        })),
        TaskStatus::NeedsHuman { reason, .. } => person.handled(reason).then(|| {
            request(ContinueTaskRequest {
                answer: Some("done".to_owned()),
                ..ContinueTaskRequest::default()
            })
        }),
        TaskStatus::NeedsInput { fields } if !fields.is_empty() => {
            let inputs = fields
                .iter()
                .map(|field| {
                    answers
                        .get(&field.name)
                        .cloned()
                        .or_else(|| person.input(field))
                        .map(|value| (field.name.clone(), value))
                })
                .collect::<Option<BTreeMap<_, _>>>()?;
            Some(request(ContinueTaskRequest {
                inputs,
                ..ContinueTaskRequest::default()
            }))
        }
        _ => None,
    }
}

/// The person at this terminal, asked on standard input. End of input
/// answers no, so a run with nobody at the terminal stops rather than waits.
#[derive(Debug, Default, Clone, Copy)]
pub struct Terminal;

impl Terminal {
    fn ask(prompt: &str) -> Option<String> {
        print!("{prompt}");
        io::stdout().flush().ok()?;
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_owned()),
        }
    }
}

impl Person for Terminal {
    fn approve(&self, action: &str, target: &str) -> bool {
        Self::ask(&format!("  approve: {action} ({target})? [y/N] ")).is_some_and(|answer| {
            answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
        })
    }

    fn handled(&self, reason: &str) -> bool {
        Self::ask(&format!(
            "  {reason}. Do it in the browser window, then press Enter (or type stop): "
        ))
        .is_some_and(|answer| !answer.eq_ignore_ascii_case("stop"))
    }

    fn input(&self, field: &InputField) -> Option<String> {
        let why = if field.why.is_empty() {
            String::new()
        } else {
            format!(" ({})", field.why)
        };
        Self::ask(&format!("  {}{why}: ", field.name)).filter(|value| !value.is_empty())
    }

    fn finish(&self, reason: &str) {
        let _ = Self::ask(&format!(
            "  {reason} The page stays open for you; press Enter when you are done: "
        ));
    }
}
