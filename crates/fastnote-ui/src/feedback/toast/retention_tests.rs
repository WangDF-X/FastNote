use super::*;

fn request(message: &str) -> ToastRequest {
    ToastRequest {
        kind: ToastKind::Success,
        message: message.to_owned(),
        action: None,
    }
}

#[test]
fn test_take_action_returns_command_and_closes_toast() {
    let mut queue = ToastQueue::new();
    let mut value = request("已创建任务");
    value.action = Some(ToastAction {
        label: "查看".to_owned(),
        command: "navigate:今日任务".to_owned(),
    });
    let PushOutcome::Shown(id) = queue.push(value) else {
        panic!("toast should be shown immediately");
    };
    assert_eq!(queue.take_action(id).as_deref(), Some("navigate:今日任务"));
    assert!(queue.is_empty());
}

#[test]
fn test_take_action_without_action_keeps_toast() {
    let mut queue = ToastQueue::new();
    let PushOutcome::Shown(id) = queue.push(request("已切换")) else {
        panic!("toast should be shown immediately");
    };
    assert_eq!(queue.take_action(id), None);
    assert_eq!(queue.visible().len(), 1);
}

#[test]
fn test_hidden_retention_is_bounded_and_keeps_visible_ids_and_recent_pending_fifo() {
    let mut queue = ToastQueue::new();
    for index in 0..500 {
        queue.push(request(&format!("安全反馈 {index}")));
    }
    let visible: Vec<_> = queue.visible().iter().map(|item| item.id).collect();
    queue.retain_for_unload();
    assert_eq!(
        queue
            .visible()
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        visible
    );
    assert_eq!(
        queue.visible.len() + queue.pending.len(),
        MAX_RETAINED_TOASTS
    );
    assert!(queue.recent.len() <= MAX_RETAINED_TOASTS);
    assert!(queue.pending.front().unwrap().message.ends_with("471"));
    assert!(queue.pending.back().unwrap().message.ends_with("499"));
}

#[test]
fn test_retention_does_not_restart_remaining_time_and_releases_old_hover_pause() {
    let mut queue = ToastQueue::new();
    let PushOutcome::Shown(id) = queue.push(request("已保存")) else {
        panic!("shown");
    };
    queue.tick(1000);
    queue.set_hovered(id, true);
    queue.retain_for_unload();
    assert!(!queue.tick(1999));
    assert_eq!(queue.visible()[0].id, id);
    assert!(queue.tick(1));
    assert!(queue.is_empty());
}

#[test]
fn test_repeated_hidden_feedback_remains_bounded_without_executing_actions() {
    let mut queue = ToastQueue::new();
    for index in 0..500 {
        let mut value = request(&format!("安全反馈 {index}"));
        value.action = Some(ToastAction {
            label: "查看".to_owned(),
            command: "navigate:今日任务".to_owned(),
        });
        queue.push(value);
        queue.retain_for_unload();
        assert!(queue.visible.len() + queue.pending.len() <= MAX_RETAINED_TOASTS);
    }
    assert_eq!(
        queue.visible()[0].action.as_ref().unwrap().command,
        "navigate:今日任务"
    );
}
