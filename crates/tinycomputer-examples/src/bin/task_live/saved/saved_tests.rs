//! Tests for what a `task_live` run keeps: its learned elements, merged
//! into the memory file and saved back.

use serde_json::json;
use tinycomputer_examples::host::LabError;

use super::{merge_memory, read_memory, remember};

fn hint(key: &str, name: &str) -> tinycomputer_bus::GroundingHint {
    tinycomputer_bus::GroundingHint {
        app: "browser".to_owned(),
        key: key.to_owned(),
        role: "button".to_owned(),
        name: Some(name.to_owned()),
        path: Vec::new(),
    }
}

#[test]
fn a_run_s_learned_elements_replace_the_same_ones_and_keep_the_rest() {
    let kept = vec![hint("search", "Go"), hint("add to cart", "Add to Cart")];
    let learned = vec![
        hint("add to cart", "Add to Bag"),
        hint("open the cart", "Cart"),
    ];
    let merged = merge_memory(kept, learned);
    let names = merged
        .iter()
        .map(|hint| hint.name.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(names, ["Go", "Add to Bag", "Cart"]);
}

#[test]
fn memory_is_read_from_its_file_and_a_run_s_learned_elements_are_saved_to_it()
-> Result<(), LabError> {
    let dir = std::env::temp_dir().join(format!("task-live-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("memory.json");
    assert!(
        read_memory(&path)?.is_empty(),
        "no file yet: nothing learned"
    );

    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string(&json!({"learned": [hint("search", "Go")]}))?,
    )?;
    remember(&dir, &path)?;
    assert_eq!(read_memory(&path)?, vec![hint("search", "Go")]);

    // A report that learned nothing keeps what the memory held.
    std::fs::write(dir.join("report.json"), "{}")?;
    remember(&dir, &path)?;
    assert_eq!(read_memory(&path)?.len(), 1);

    // A memory in a folder not made yet is saved there.
    let nested = dir.join("memory").join("shop.json");
    remember(&dir, &nested)?;
    assert_eq!(read_memory(&nested)?.len(), 0);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
