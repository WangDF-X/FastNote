use super::*;

#[test]
fn test_released_models_clear_every_bound_alias_without_creating_a_window() {
    let presenter = TodayPresenter {
        sections: Rc::new(VecModel::default()),
        rows: std::array::from_fn(|_| Rc::new(VecModel::default())),
    };
    for rows in &presenter.rows {
        rows.set_vec(
            (0..1000)
                .map(|index| TaskRowData {
                    title: format!("假任务 {index}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        );
    }
    let alias = ModelRc::from(presenter.rows[0].clone());
    let weak = Rc::downgrade(&presenter.rows[0]);
    presenter.sections.push(TodaySectionData {
        items: alias.clone(),
        ..Default::default()
    });
    assert_eq!(alias.row_count(), 1000);
    presenter.release_models();
    assert_eq!(alias.row_count(), 0);
    assert_eq!(presenter.sections.row_count(), 0);
    assert!(presenter.rows.iter().all(|rows| rows.row_count() == 0));
    drop(alias);
    drop(presenter);
    assert!(weak.upgrade().is_none());
}

#[test]
fn test_page_and_clean_editor_release_unbind_data_instead_of_only_hiding_a_drawer() {
    let source = include_str!("../presentation.rs");
    let release = source
        .split_once("fn release(")
        .unwrap()
        .1
        .split_once("fn release_models(")
        .unwrap()
        .0;
    assert!(release.contains("set_today_mounted(false)"));
    assert!(release.contains("set_today_sections(ModelRc::default())"));
    let session = include_str!("../../../../fastnote-app/src/desktop/ui_session.rs");
    let editor = session
        .split_once("fn release_editor_resources(")
        .unwrap()
        .1
        .split_once("fn apply_today(")
        .unwrap()
        .0;
    assert!(editor.contains("set_task_description(\"\".into())"));
    assert!(editor.contains("set_task_title(\"\".into())"));
    assert!(!editor.contains("task_quick_title"));
}
