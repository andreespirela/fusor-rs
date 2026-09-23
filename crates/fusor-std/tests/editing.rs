#![cfg(all(feature = "forms", feature = "actions"))]

use fusor::{Owner, Signal, effect, signal};
use fusor_std::{
    actions::{Action, AdmissionError, Outcome, SavePolicy, Status},
    forms::{Acknowledgment, Form, FormError, SubmissionStatus, TextField},
};
use fusor_test::{ControlledLoader, TestExecutor};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Debug, PartialEq)]
struct Command {
    id: u64,
    expected: u64,
    title: String,
    count: u32,
}
#[derive(Debug)]
struct Saved {
    version: u64,
    title: String,
    count: u32,
}
type EditorForm = Form<(TextField<String>, TextField<u32>), Command>;
type Save = Action<Command, Saved, String>;
type Backend = ControlledLoader<Rc<Command>, Outcome<Saved, String>, ()>;

struct Editor {
    owner: Rc<Owner>,
    executor: TestExecutor,
    backend: Backend,
    title: TextField<String>,
    count: TextField<u32>,
    version: Signal<u64>,
    form: EditorForm,
    save: Save,
}
impl Editor {
    fn new() -> Self {
        let owner = Rc::new(Owner::new());
        let executor = TestExecutor::new();
        let backend = Backend::new();
        let title = TextField::new("Thursday".to_owned()).validate(|value| {
            if value.trim().is_empty() {
                Err("Enter a title".into())
            } else {
                Ok(())
            }
        });
        let count = TextField::new(1_u32);
        let version = signal(10);
        let captured = version.clone();
        let form = Form::new(
            &owner.handle(),
            "project:7",
            (title.clone(), count.clone()),
            move |(title, count)| Command {
                id: 7,
                expected: captured.get_untracked(),
                title,
                count,
            },
        )
        .unwrap();
        let loader = backend.clone();
        let save = Action::new(
            &owner.handle(),
            SavePolicy::RejectWhilePending,
            move |command, context| {
                let future = loader.load(command, context);
                async move { future.await.unwrap() }
            },
            executor.spawner(),
        );
        owner.commit();
        Self {
            owner,
            executor,
            backend,
            title,
            count,
            version,
            form,
            save,
        }
    }
    fn mapping(&self) -> impl FnOnce(&mut Acknowledgment, &Saved) + 'static {
        let title = self.title.clone();
        let count = self.count.clone();
        let version = self.version.clone();
        move |ack, saved| {
            ack.field(&title, saved.title.clone());
            ack.field(&count, saved.count);
            ack.version(&version, saved.version);
        }
    }
    fn submit(&self) {
        self.form
            .submit(&self.save, self.form.prepare().unwrap(), self.mapping())
            .unwrap();
        self.executor.run_until_stalled();
    }
    fn complete(&self, outcome: Outcome<Saved, String>) {
        self.backend
            .next_request()
            .unwrap()
            .complete(Ok(outcome))
            .unwrap();
        self.executor.run_until_stalled();
    }
}

#[test]
fn friday_then_monday_preserves_newer_draft_and_publishes_coherently() {
    let e = Editor::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let rows = observed.clone();
    let title = e.title.clone();
    let version = e.version.clone();
    let save = e.save.clone();
    let form = e.form.clone();
    let _observer = effect(move || {
        rows.borrow_mut().push((
            title.raw(),
            title.baseline(),
            version.get(),
            save.state().status,
            form.status(),
        ))
    });
    e.title.edit("Friday");
    e.count.edit("2");
    e.submit();
    let request = e.backend.next_request().unwrap();
    assert_eq!(
        *request.key,
        Command {
            id: 7,
            expected: 10,
            title: "Friday".into(),
            count: 2
        }
    );
    e.title.edit("Monday");
    assert!(e.form.submission_message().contains("earlier edit"));
    let snapshot = e.form.prepare().unwrap();
    let id = snapshot.id();
    let error = e.form.submit(&e.save, snapshot, e.mapping()).unwrap_err();
    assert_eq!(error.reason, FormError::Busy);
    assert_eq!(error.snapshot.id(), id);
    assert_eq!(e.save.state().submission.unwrap().command.title, "Friday");
    observed.borrow_mut().clear();
    request
        .complete(Ok(Outcome::Accepted(Saved {
            title: "Friday".into(),
            count: 2,
            version: 11,
        })))
        .unwrap();
    e.executor.run_until_stalled();
    assert_eq!(
        *observed.borrow(),
        [(
            "Monday".into(),
            "Friday".into(),
            11,
            Status::Accepted,
            SubmissionStatus::Accepted
        )]
    );
    assert!(e.form.dirty());
    assert!(!e.count.dirty());
    e.submit();
    assert_eq!(e.save.state().submission.unwrap().command.expected, 11);
    e.complete(Outcome::Accepted(Saved {
        title: "Monday".into(),
        count: 2,
        version: 12,
    }));
    assert!(!e.form.dirty());
    assert_eq!(e.form.submission_message(), "Saved");
    assert_eq!(e.backend.counts().cancelled, 0);
}

#[test]
fn normalization_and_edit_reversion_use_raw_baseline_equality() {
    let e = Editor::new();
    e.title.edit(" Friday ");
    e.submit();
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(e.title.raw(), "Friday");
    assert!(!e.title.dirty());
    e.title.edit("Monday");
    e.submit();
    e.title.edit("Tuesday");
    e.title.edit("Monday");
    e.complete(Outcome::Accepted(Saved {
        title: "Monday".into(),
        count: 1,
        version: 12,
    }));
    assert!(!e.title.dirty());
}

#[test]
fn invalid_drafts_remain_visible_and_cross_field_validation_is_scoped() {
    let e = Editor::new();
    e.count.edit("-");
    assert!(e.count.parsed().is_err());
    assert_eq!(e.count.message(), "");
    assert!(matches!(e.form.prepare(), Err(FormError::Invalid)));
    assert_eq!(e.count.raw(), "-");
    assert!(!e.count.message().is_empty());
    assert_eq!(e.backend.counts().started, 0);
    e.count.edit("2");
    e.title.edit("restricted");
    let form = e.form.clone().validate(|(title, count)| {
        if title == "restricted" && *count > 1 {
            Err("Only one allowed".into())
        } else {
            Ok(())
        }
    });
    assert!(matches!(form.prepare(), Err(FormError::Invalid)));
    assert_eq!(form.validation_message(), "Only one allowed");
    e.count.edit("1");
    assert_eq!(form.validation_message(), "");
    assert!(form.prepare().is_ok());
}

#[test]
fn preparation_detects_reentrant_builder_and_validator_edits() {
    let e = Editor::new();
    let title = e.title.clone();
    let form = Form::new(
        &e.owner.handle(),
        "7",
        (e.title.clone(),),
        move |(value,)| {
            title.edit("changed");
            value
        },
    )
    .unwrap();
    assert!(matches!(
        form.prepare(),
        Err(FormError::ChangedDuringPreparation)
    ));
    let count = e.count.clone();
    let form = e.form.clone().validate(move |_| {
        count.edit("3");
        Ok(())
    });
    assert!(matches!(
        form.prepare(),
        Err(FormError::ChangedDuringPreparation)
    ));
}

#[test]
fn stale_foreign_and_reconfigured_snapshots_do_not_dispatch() {
    let e = Editor::new();
    let snapshot = e.form.prepare().unwrap();
    e.title.edit("newer");
    assert_eq!(
        e.form
            .submit(&e.save, snapshot, e.mapping())
            .unwrap_err()
            .reason,
        FormError::StaleSnapshot
    );
    let other = Editor::new();
    assert_eq!(
        e.form
            .submit(&e.save, other.form.prepare().unwrap(), e.mapping())
            .unwrap_err()
            .reason,
        FormError::WrongForm
    );
    let snapshot = e.form.prepare().unwrap();
    let _field = e.title.clone().validate(|_| Ok(()));
    assert_eq!(
        e.form
            .submit(&e.save, snapshot, e.mapping())
            .unwrap_err()
            .reason,
        FormError::StaleSnapshot
    );
    assert_eq!(e.backend.counts().started, 0);
    assert!(matches!(
        Form::new(
            &e.owner.handle(),
            "7",
            (e.title.clone(), e.title.clone()),
            |_| ()
        ),
        Err(FormError::DuplicateField)
    ));
}

#[test]
fn every_invalid_mapping_aborts_the_entire_local_publication() {
    for scenario in 0..5 {
        let e = Editor::new();
        e.title.edit("Friday");
        let title = e.title.clone();
        let count = e.count.clone();
        let version = e.version.clone();
        let foreign = TextField::new("other".to_owned());
        let after = Rc::new(Cell::new(false));
        let after_copy = after.clone();
        e.form
            .submit(&e.save, e.form.prepare().unwrap(), move |ack, saved| {
                ack.field(&title, saved.title.clone());
                ack.field(&count, saved.count);
                ack.version(&version, saved.version);
                match scenario {
                    0 => ack.field(&foreign, "invalid".into()),
                    1 => ack.field(&title, "duplicate".into()),
                    2 => ack.version(&version, 9),
                    3 => title.reset("reviewed".into()),
                    _ => ack.reject("wrong response entity"),
                }
                ack.after_commit(move || after_copy.set(true));
            })
            .unwrap();
        e.executor.run_until_stalled();
        e.complete(Outcome::Accepted(Saved {
            title: "Friday".into(),
            count: 9,
            version: 11,
        }));
        assert_eq!(
            e.title.baseline(),
            if scenario == 3 {
                "reviewed"
            } else {
                "Thursday"
            }
        );
        assert_eq!(e.count.baseline(), "1");
        assert_eq!(e.version.get(), 10);
        assert_eq!(e.form.status(), SubmissionStatus::PublicationFailed);
        assert_eq!(e.save.state().status, Status::PublicationFailed);
        assert!(!after.get());
        assert_eq!(
            e.form
                .submit(&e.save, e.form.prepare().unwrap(), e.mapping())
                .unwrap_err()
                .reason,
            FormError::Unresolved
        );
    }
}

#[test]
fn rejection_errors_expire_when_any_submitted_dependency_changes() {
    let e = Editor::new();
    e.title.edit("Friday");
    let title = e.title.clone();
    e.form
        .submit_with(
            &e.save,
            e.form.prepare().unwrap(),
            e.mapping(),
            move |errors, error| {
                errors.field(&title, error);
                errors.form("Review both fields");
            },
        )
        .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Rejected("Reserved title".into()));
    assert_eq!(e.title.message(), "Reserved title");
    assert_eq!(e.form.validation_message(), "Review both fields");
    e.count.edit("2");
    assert_eq!(e.title.message(), "");
    assert_eq!(e.form.validation_message(), "");
    e.submit();
    e.title.edit("Monday");
    e.complete(Outcome::Rejected("Old error".into()));
    assert_eq!(e.title.raw(), "Monday");
    assert_eq!(e.title.message(), "");
}

#[test]
fn acceptance_does_not_erase_newer_cross_field_validation() {
    let e = Editor::new();
    let form = e.form.clone().validate(|(title, _)| {
        if title == "invalid" {
            Err("Newer validation".into())
        } else {
            Ok(())
        }
    });
    e.title.edit("Friday");
    e.submit();
    e.title.edit("invalid");
    assert!(matches!(form.prepare(), Err(FormError::Invalid)));
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(form.validation_message(), "Newer validation");
}

#[test]
fn newer_validation_of_unchanged_values_survives_an_older_acknowledgment() {
    let e = Editor::new();
    e.submit();
    let form = e
        .form
        .clone()
        .validate(|_| Err("Updated validation policy".into()));
    assert!(matches!(form.prepare(), Err(FormError::Invalid)));
    e.complete(Outcome::Accepted(Saved {
        title: "Thursday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(form.validation_message(), "Updated validation policy");
}

#[test]
fn normalization_expires_validation_of_the_replaced_raw_value() {
    let e = Editor::new();
    e.title.edit(" Friday ");
    e.submit();
    let form = e.form.clone().validate(|(title, _)| {
        if title.starts_with(' ') {
            Err("Leading space".into())
        } else {
            Ok(())
        }
    });
    assert!(matches!(form.prepare(), Err(FormError::Invalid)));
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(e.title.raw(), "Friday");
    assert_eq!(form.validation_message(), "");
}

#[test]
fn conflict_and_unknown_retain_the_command_until_explicit_reconciliation() {
    for outcome in [
        Outcome::Conflict("version mismatch".into()),
        Outcome::Unknown("connection lost".into()),
    ] {
        let e = Editor::new();
        e.title.edit("Friday");
        e.submit();
        e.complete(outcome);
        assert!(e.form.status().unresolved());
        assert!(e.save.state().status.unresolved());
        assert_eq!(e.save.state().submission.unwrap().command.title, "Friday");
        assert_eq!(
            e.form
                .submit(&e.save, e.form.prepare().unwrap(), e.mapping())
                .unwrap_err()
                .reason,
            FormError::Unresolved
        );
        assert_eq!(e.backend.counts().started, 1);
        assert_eq!(e.title.baseline(), "Thursday");
        let other = Editor::new();
        assert_eq!(e.form.reconcile(&other.save), Err(FormError::WrongAction));
        // Application reviews authoritative data, then explicitly unlocks saves.
        e.title.reset("Friday".into());
        e.version.set(11);
        e.form.reconcile(&e.save).unwrap();
        assert_eq!(e.save.state().status, Status::Idle);
        assert!(e.save.state().submission.is_none());
        e.submit();
        assert_eq!(e.save.state().submission.unwrap().command.expected, 11);
    }
}

#[test]
fn prepared_owner_has_no_io_and_disposal_releases_futures_at_executor_boundary() {
    let e = Editor::new();
    let prepared = Owner::child(&e.owner.handle());
    let backend = e.backend.clone();
    let save = Action::new(
        &prepared.handle(),
        SavePolicy::RejectWhilePending,
        move |command, context| {
            let future = backend.load(command, context);
            async move { future.await.unwrap() }
        },
        e.executor.spawner(),
    );
    let snapshot = e.form.prepare().unwrap();
    let id = snapshot.id();
    let denied = e.form.submit(&save, snapshot, e.mapping()).unwrap_err();
    assert_eq!(denied.reason, FormError::NotActive);
    assert_eq!(denied.snapshot.id(), id);
    assert_eq!(e.form.status(), SubmissionStatus::Idle);
    prepared.commit();
    e.form.submit(&save, denied.snapshot, e.mapping()).unwrap();
    e.executor.run_until_stalled();
    let pending = e.backend.next_request().unwrap();
    assert_eq!(e.backend.counts().live, 1);
    save.dispose();
    assert!(pending.is_cancelled());
    assert_eq!(e.form.status(), SubmissionStatus::Disposed);
    assert_eq!(e.backend.counts().live, 1);
    e.executor.run_until_stalled();
    assert_eq!(e.backend.counts().live, 0);
    assert!(
        pending
            .complete(Ok(Outcome::Accepted(Saved {
                title: "late".into(),
                count: 1,
                version: 99
            })))
            .is_err()
    );
    assert_eq!(e.title.raw(), "Thursday");
    assert_eq!(e.version.get(), 10);
}

#[test]
fn retained_session_survives_view_disposal_but_not_application_disposal() {
    let e = Editor::new();
    let view = Owner::child(&e.owner.handle());
    view.commit();
    e.title.edit("Friday");
    e.submit();
    view.dispose();
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(e.version.get(), 11);
    e.title.edit("Monday");
    e.submit();
    let request = e.backend.next_request().unwrap();
    request
        .complete(Ok(Outcome::Accepted(Saved {
            title: "Monday".into(),
            count: 1,
            version: 12,
        })))
        .unwrap();
    e.owner.dispose();
    e.executor.run_until_stalled();
    assert_eq!(e.form.status(), SubmissionStatus::Disposed);
    assert_eq!(e.save.state().status, Status::Disposed);
    assert_eq!(e.version.get(), 11);
}

#[test]
fn reentrant_mapping_is_busy_and_after_commit_can_start_the_next_save() {
    let e = Editor::new();
    e.title.edit("Friday");
    let save = e.save.clone();
    let form = e.form.clone();
    let mapping = e.mapping();
    let next_mapping = e.mapping();
    let version = e.version.clone();
    let title = e.title.clone();
    e.form
        .submit(&e.save, e.form.prepare().unwrap(), move |ack, saved| {
            assert_eq!(
                save.dispatch(Command {
                    id: 7,
                    expected: 10,
                    title: "wrong".into(),
                    count: 1
                })
                .unwrap_err()
                .reason,
                AdmissionError::Busy
            );
            mapping(ack, saved);
            ack.after_commit(move || {
                assert_eq!(version.get(), 11);
                assert_eq!(form.status(), SubmissionStatus::Accepted);
                assert_eq!(save.state().status, Status::Accepted);
                title.edit("Monday");
                form.submit(&save, form.prepare().unwrap(), next_mapping)
                    .unwrap();
            });
        })
        .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert_eq!(e.save.state().status, Status::Pending);
    assert_eq!(e.backend.counts().started, 2);
    e.complete(Outcome::Accepted(Saved {
        title: "Monday".into(),
        count: 1,
        version: 12,
    }));
    assert_eq!(e.version.get(), 12);
    assert_eq!(e.backend.counts().cancelled, 0);
}

#[test]
fn disposal_during_mapping_suppresses_every_staged_write() {
    let e = Editor::new();
    let owner = e.owner.clone();
    let mapping = e.mapping();
    e.form
        .submit(&e.save, e.form.prepare().unwrap(), move |ack, saved| {
            mapping(ack, saved);
            owner.dispose();
        })
        .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Accepted(Saved {
        title: "late".into(),
        count: 2,
        version: 11,
    }));
    assert_eq!(e.version.get(), 10);
    assert_eq!(e.title.raw(), "Thursday");
    assert_eq!(e.form.status(), SubmissionStatus::Disposed);
}

struct OnDrop(Option<Box<dyn FnOnce()>>);
impl Drop for OnDrop {
    fn drop(&mut self) {
        if let Some(callback) = self.0.take() {
            callback();
        }
    }
}

#[test]
fn unused_mapper_captures_drop_before_acknowledgment_validation() {
    let e = Editor::new();
    let title = e.title.clone();
    let guard = OnDrop(Some(Box::new(move || title.reset("Reviewed".into()))));
    e.form
        .submit_with(
            &e.save,
            e.form.prepare().unwrap(),
            e.mapping(),
            move |_, _| {
                let _ = &guard;
            },
        )
        .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Accepted(Saved {
        title: "Stale".into(),
        count: 2,
        version: 11,
    }));
    assert_eq!(e.title.raw(), "Reviewed");
    assert_eq!(e.title.baseline(), "Reviewed");
    assert_eq!(e.version.get(), 10);
    assert_eq!(e.form.status(), SubmissionStatus::PublicationFailed);
}

#[test]
fn discarded_mappings_cannot_revive_a_disposed_separate_form_owner() {
    let e = Editor::new();
    let owner = Rc::new(Owner::child(&e.owner.handle()));
    owner.commit();
    let form = Form::new(
        &owner.handle(),
        "project:7",
        (e.title.clone(), e.count.clone()),
        |(title, count)| Command {
            id: 7,
            expected: 10,
            title,
            count,
        },
    )
    .unwrap();
    let guard = OnDrop(Some(Box::new(move || owner.dispose())));
    let title = e.title.clone();
    form.submit(&e.save, form.prepare().unwrap(), move |ack, saved| {
        ack.field(&title, saved.title.clone());
        ack.field(&title, "Duplicate".into());
        ack.after_commit(move || drop(guard));
    })
    .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Accepted(Saved {
        title: "Stale".into(),
        count: 2,
        version: 11,
    }));
    assert_eq!(form.status(), SubmissionStatus::Disposed);
    assert_eq!(e.title.baseline(), "Thursday");
    assert_eq!(e.save.state().status, Status::PublicationFailed);
}

#[test]
fn replaced_user_values_drop_only_after_fields_version_and_status_agree() {
    let e = Editor::new();
    let mapping = e.mapping();
    let title = e.title.clone();
    let version = e.version.clone();
    let form = e.form.clone();
    let save = e.save.clone();
    let dropped = Rc::new(Cell::new(false));
    let called = dropped.clone();
    let arbitrary = signal(OnDrop(Some(Box::new(move || {
        assert_eq!(title.baseline(), "Friday");
        assert_eq!(version.get(), 11);
        assert_eq!(form.status(), SubmissionStatus::Accepted);
        assert_eq!(save.state().status, Status::Accepted);
        called.set(true);
    }))));
    e.form
        .submit(&e.save, e.form.prepare().unwrap(), move |ack, saved| {
            mapping(ack, saved);
            ack.signal(&arbitrary, OnDrop(None));
        })
        .unwrap();
    e.executor.run_until_stalled();
    e.complete(Outcome::Accepted(Saved {
        title: "Friday".into(),
        count: 1,
        version: 11,
    }));
    assert!(dropped.get());
}
