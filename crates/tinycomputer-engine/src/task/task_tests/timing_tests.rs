//! Tests for what a task journals of its time outside its flows.

use std::time::Duration;

use tinycomputer_bus::agent::{
    LanguageModelConfiguration, LanguageModelProvider, Rescue, RescueOutcome,
};

use super::super::timing::{Rescued, answered, planned, rescued, resumed};
use crate::planner::ModelUse;
use crate::rescue::Guidance;

#[test]
fn a_rescue_answer_is_named_for_how_it_went() {
    let retry = Ok(Guidance::Retry {
        reason: String::new(),
        steps: Vec::new(),
        covers: 0,
    });
    assert_eq!(answered(&retry, false), "guided");
    let give_up = Ok(Guidance::GiveUp {
        reason: String::new(),
    });
    assert_eq!(answered(&give_up, false), "gave_up");
    let late = Err("the rescuer took too long".to_owned());
    assert_eq!(answered(&late, true), "timeout");
    assert_eq!(answered(&late, false), "error");
}

#[test]
fn a_rescue_with_no_answer_in_time_journals_no_model_use() {
    let record = Rescue {
        step: 4,
        failure: "nothing to click".to_owned(),
        reason: "the rescuer took too long".to_owned(),
        steps: Vec::new(),
        covers: 0,
        outcome: RescueOutcome::GaveUp,
    };
    let model = LanguageModelConfiguration {
        provider: LanguageModelProvider::OpenRouter,
        model: "openrouter/deepseek/deepseek-v4-flash".to_owned(),
        endpoint_url: None,
    };
    let fields = rescued(&Rescued {
        attempt: 2,
        limit: 5,
        took: Duration::from_millis(1500),
        used: None,
        outcome: "timeout",
        record: &record,
        model: Some(&model),
    });
    assert_eq!(fields["step"], 5, "steps count from 1");
    assert_eq!(fields["wall_ms"], 1500);
    assert!(fields["calls"].is_null() && fields["sent_bytes"].is_null());
    assert_eq!(fields["model"], "openrouter/deepseek/deepseek-v4-flash");

    let plan = planned(
        &Err("down".to_owned()),
        ModelUse {
            calls: 3,
            sent_bytes: 10,
        },
        Duration::from_secs(2),
        None,
    );
    assert_eq!(plan["calls"], 3);
    assert!(plan["model"].is_null());
    assert!(plan.get("steps").is_none());

    assert_eq!(
        resumed("needs_approval", Duration::from_secs(9)),
        serde_json::json!({"state": "needs_approval", "waited_ms": 9000})
    );
}
