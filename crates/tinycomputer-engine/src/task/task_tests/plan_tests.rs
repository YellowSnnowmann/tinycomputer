//! Tests for planning a plain-language task before running it.

use super::*;

/// A model that always answers with the same text.
struct Fixed(Result<String, String>);

impl crate::planner::LanguageModel for Fixed {
    fn complete(&self, _turns: &[crate::planner::Turn]) -> crate::planner::Completion {
        let answer = self.0.clone();
        Box::pin(async move { answer })
    }
}

fn planned(replies: Vec<DesktopResponse>, answer: Result<&str, &str>) -> (Tasks, Arc<Script>) {
    let (tasks, script) = controller(replies);
    let model = Arc::new(Fixed(answer.map(str::to_owned).map_err(str::to_owned)));
    (
        tasks.with_planner(crate::planner::Planner::new(model)),
        script,
    )
}

#[tokio::test]
async fn a_plain_language_task_is_planned_then_run() {
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(r#"{"app": "Mail", "steps": [{"enter": {"recipient": "${email}"}}]}"#),
    );
    assert!(tasks.planner_configured());
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("email Sam".to_owned()),
            facts: BTreeMap::from([("email".to_owned(), "sam@example.com".to_owned())]),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert_eq!(started.summary, "Planning the task.");
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Done { .. }
    ));
    let requests = script.requests.lock().unwrap();
    assert_eq!(requests[0].flow.app, "Mail");
    assert_eq!(requests[0].vars["email"], "sam@example.com");
    assert_eq!(
        tasks
            .report(&started.id)
            .data
            .unwrap()
            .flow
            .unwrap()
            .steps
            .len(),
        1
    );
    let plans = journaled(&script, "plan");
    assert_eq!(plans.len(), 1);
    assert_eq!(
        plans[0].0.as_ref(),
        Some(&started.id),
        "into the task's journal"
    );
    assert_eq!(plans[0].1["ok"], true);
    assert_eq!(plans[0].1["calls"], 1);
    assert_eq!(plans[0].1["steps"], 1);
    assert!(plans[0].1["wall_ms"].is_u64());
}

#[tokio::test]
async fn a_plan_that_needs_values_asks_and_a_failed_plan_says_so() {
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(r#"{"app": "browser", "steps": [{"enter": {"phone": "${phone}"}}]}"#),
    );
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("fill my phone".to_owned()),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    let waiting = settle(&tasks, &started.id).await;
    assert!(
        matches!(waiting.status, TaskStatus::NeedsInput { ref fields } if fields[0].name == "phone")
    );
    assert!(
        tasks
            .continue_task(ContinueTaskRequest {
                id: started.id.clone(),
                inputs: BTreeMap::from([("phone".to_owned(), "+91 98765 43210".to_owned())]),
                ..ContinueTaskRequest::default()
            })
            .ok
    );
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Done { .. }
    ));
    assert_eq!(
        script.requests.lock().unwrap()[0].vars["phone"],
        "+91 98765 43210"
    );
    let resumes = journaled(&script, "resume");
    assert_eq!(
        resumes.len(),
        1,
        "the answer journals how long the task waited"
    );
    assert_eq!(resumes[0].1["state"], "needs_input");
    assert!(resumes[0].1["waited_ms"].is_u64());

    let (tasks, _) = planned(Vec::new(), Err("the model is down"));
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("anything".to_owned()),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Failed { ref reason, recoverable: true, .. } if reason == "the model is down"
    ));
}

#[tokio::test]
async fn plan_task_drafts_without_acting() {
    use tinycomputer_bus::agent::PlanTaskRequest;

    let request = PlanTaskRequest {
        task: "email Sam".to_owned(),
        ..PlanTaskRequest::default()
    };
    let (tasks, _) = controller(Vec::new());
    assert_eq!(code(&tasks.plan(&request).await), "PLANNER_NOT_CONFIGURED");
    let (tasks, script) = planned(
        Vec::new(),
        Ok(r#"{"app": "Mail", "steps": ["start a new email message"]}"#),
    );
    assert_eq!(tasks.plan(&request).await.data.unwrap().flow.app, "Mail");
    assert!(
        script.requests.lock().unwrap().is_empty(),
        "planning never runs anything"
    );
    let plans = journaled(&script, "plan");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].0, None, "no task yet: a run of its own");
    assert_eq!(plans[0].1["ok"], true);
    let (tasks, script) = planned(Vec::new(), Err("down"));
    assert_eq!(code(&tasks.plan(&request).await), "PLAN_FAILED");
    let plans = journaled(&script, "plan");
    assert_eq!(plans[0].1["ok"], false);
    assert_eq!(plans[0].1["error"], "down");
}

#[tokio::test]
async fn a_browser_only_task_gets_its_browser_ready_while_it_is_planned() {
    use tinycomputer_bus::agent::{SurfaceKind, TaskConstraints};

    let mail = r#"{"app": "browser", "steps": [{"browse": "https://mail.test"}]}"#;
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(mail),
    );
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("open my mail".to_owned()),
            constraints: TaskConstraints {
                surfaces: vec![SurfaceKind::Browser],
                ..TaskConstraints::default()
            },
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Done { .. }
    ));
    assert_eq!(
        *script.prepared.lock().unwrap(),
        std::slice::from_ref(&started.id)
    );

    // A task that may also use the desktop could be planned for either, so
    // nothing is opened before the plan says which.
    let (tasks, script) = planned(
        vec![finished_run(FlowStopReason::Completed, vec![], &[], None)],
        Ok(mail),
    );
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("open my mail".to_owned()),
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    settle(&tasks, &started.id).await;
    assert!(script.prepared.lock().unwrap().is_empty());

    // A plan that fails lets go of what was made ready for it.
    let (tasks, script) = planned(Vec::new(), Err("the model is down"));
    let started = tasks
        .start(&StartTaskRequest {
            task: Some("open my mail".to_owned()),
            constraints: TaskConstraints {
                surfaces: vec![SurfaceKind::Browser],
                ..TaskConstraints::default()
            },
            ..StartTaskRequest::default()
        })
        .data
        .unwrap();
    assert!(matches!(
        settle(&tasks, &started.id).await.status,
        TaskStatus::Failed { .. }
    ));
    assert_eq!(
        *script.prepared.lock().unwrap(),
        std::slice::from_ref(&started.id)
    );
    assert_eq!(
        *script.released.lock().unwrap(),
        std::slice::from_ref(&started.id)
    );
}
