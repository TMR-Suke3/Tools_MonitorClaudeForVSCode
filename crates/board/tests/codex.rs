use mcv_board::watch::codex::{Activity, CodexWatch, matching_window, session_uri};
use std::fs::{self, OpenOptions};
use std::io::Write;

#[test]
fn follows_distinct_editor_sessions_and_expires_finished_rows() {
    let root = std::env::temp_dir().join(format!("mcv-codex-test-{}", std::process::id()));
    let sessions = root.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let ids = [
        "01900000-0000-7000-8000-000000000001",
        "01900000-0000-7000-8000-000000000002",
        "01900000-0000-7000-8000-000000000003",
        "01900000-0000-7000-8000-000000000004",
    ];
    let now = mcv_core::time::Timestamp::parse_iso8601("2026-11-24T00:00:00.000Z")
        .unwrap()
        .as_millis();
    let fixture_time =
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(now.try_into().unwrap());
    for (index, id) in ids.iter().enumerate() {
        let source = if index == 3 { "exec" } else { "vscode" };
        let cwd = if index == 1 { "C:/other" } else { "C:/work" };
        fs::write(sessions.join(format!("rollout-{index}.jsonl")), format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"source\":\"{source}\",\"id\":\"{id}\",\"cwd\":\"{cwd}\"}}}}\n\
             {{\"type\":\"event_msg\",\"timestamp\":\"2026-11-24T00:00:00.123456Z\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"first\"}}}}\n"
        )).unwrap();
        OpenOptions::new()
            .write(true)
            .open(sessions.join(format!("rollout-{index}.jsonl")))
            .unwrap()
            .set_modified(fixture_time)
            .unwrap();
    }
    fs::write(
        root.join("session_index.jsonl"),
        format!(
            "{{\"id\":\"{}\",\"thread_name\":\"Example title\"}}\n",
            ids[0]
        ),
    )
    .unwrap();
    let mut watch = CodexWatch::new(sessions.clone());
    let rows = watch.poll(now + 124);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].title.as_deref(), Some("Example title"));
    assert_ne!(rows[0].cwd, rows[1].cwd);
    assert_eq!(rows[0].cwd, rows[2].cwd);
    assert!(rows.iter().all(|row| row.activity == Activity::Working));
    let mut file = OpenOptions::new()
        .append(true)
        .open(sessions.join("rollout-0.jsonl"))
        .unwrap();
    write!(file, r#"{{"type":"event_msg","timestamp":"2026-11-24T00:00:01.000Z","payload":{{"type":"task_complete","turn_id":"first"}}}}"#).unwrap();
    assert!(
        watch
            .poll(now + 1000)
            .iter()
            .all(|row| row.activity == Activity::Working)
    );
    writeln!(file).unwrap();
    let rows = watch.poll(now + 1001);
    assert_eq!(
        rows.iter().find(|row| row.id == ids[0]).unwrap().activity,
        Activity::Complete
    );
    assert_eq!(
        rows.iter().find(|row| row.id == ids[1]).unwrap().activity,
        Activity::Working
    );
    let rows = watch.poll(now + 301_000);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.activity == Activity::Working));
    assert!(watch.poll(now + 86_400_000).is_empty());
    drop(file);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn targets_the_local_conversation_and_rejects_invalid_ids() {
    let id = "01900000-0000-7000-8000-000000000001";
    assert_eq!(
        session_uri("vscode-insiders", id).unwrap(),
        format!("vscode-insiders://openai.chatgpt/local/{id}")
    );
    assert!(session_uri("vscode", "../new").is_none());
    assert!(session_uri("https", id).is_none());
}

#[test]
fn only_targets_an_unambiguous_workspace_window() {
    let first = "file.rs - work - Visual Studio Code - Insiders";
    let second = "work - other - Visual Studio Code - Insiders";
    assert_eq!(
        matching_window("work", [first, second].into_iter()),
        Some(0)
    );
    assert_eq!(
        matching_window("other", [first, second].into_iter()),
        Some(1)
    );
    assert_eq!(matching_window("work", [first, first].into_iter()), None);
    assert_eq!(matching_window("work", [second].into_iter()), None);
}

#[test]
fn gui_question_remains_waiting_after_ack_until_matching_reply() {
    let root = std::env::temp_dir().join(format!("mcv-codex-question-{}", std::process::id()));
    let sessions = root.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join("rollout-question.jsonl");
    fs::write(&rollout, concat!(
        r#"{"type":"session_meta","payload":{"id":"01900000-0000-7000-8000-000000000001","cwd":"C:/work","source":"vscode"}}"#, "\n",
        r#"{"type":"event_msg","timestamp":"2026-11-25T00:00:00.000Z","payload":{"type":"task_started","turn_id":"turn"}}"#, "\n",
        r#"{"type":"response_item","timestamp":"2026-11-25T00:00:01.000Z","payload":{"type":"function_call","name":"request_user_input_async","call_id":"call_test"}}"#, "\n",
        r#"{"type":"response_item","timestamp":"2026-11-25T00:00:02.000Z","payload":{"type":"function_call_output","call_id":"call_test","output":"{\"accepted\":true}"}}"#, "\n",
    )).unwrap();
    let now = mcv_core::time::Timestamp::parse_iso8601("2026-11-25T00:00:07.000Z")
        .unwrap()
        .as_millis();
    OpenOptions::new()
        .write(true)
        .open(&rollout)
        .unwrap()
        .set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(now.try_into().unwrap()),
        )
        .unwrap();
    let mut watch = CodexWatch::new(sessions);
    assert_eq!(watch.poll(now)[0].activity, Activity::AwaitingUser);
    let mut file = OpenOptions::new().append(true).open(&rollout).unwrap();
    for (call, expected) in [
        ("call_other", Activity::AwaitingUser),
        ("call_test", Activity::Working),
    ] {
        let reply = serde_json::json!({
            "type": "response_item",
            "timestamp": "2026-11-25T00:00:04.000Z",
            "payload": {
                "type": "message", "role": "user",
                "content": [{"type": "input_text", "text": format!("<send_user_message_question_reply>{}</send_user_message_question_reply>", serde_json::json!([{
                    "questionItemId": serde_json::json!(["request_user_input_async", call, 0]).to_string()
                }]))}]
            }
        });
        writeln!(file, "{reply}").unwrap();
        assert_eq!(watch.poll(now + 1000)[0].activity, expected);
    }
    file.write_all(concat!(r#"{"type":"response_item","timestamp":"2026-11-25T00:00:05.000Z","payload":{"type":"function_call","name":"request_user_input_async","call_id":"call_unanswered"}}"#, "\n").as_bytes()).unwrap();
    assert_eq!(watch.poll(now)[0].activity, Activity::AwaitingUser);
    file.write_all(concat!(r#"{"type":"event_msg","timestamp":"2026-11-25T00:00:06.000Z","payload":{"type":"task_complete","turn_id":"turn"}}"#, "\n").as_bytes()).unwrap();
    assert_eq!(watch.poll(now)[0].activity, Activity::Complete);
    drop(file);
    fs::remove_dir_all(root).unwrap();
}
