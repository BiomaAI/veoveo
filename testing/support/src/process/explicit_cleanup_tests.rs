//! Actual admitted subprocesses, isolated from other tests' global owner state.
use super::*;
use std::time::Instant;

#[test]
fn async_cleanup_keeps_original_owner_cap_and_failed_identity() {
    const KEY: &str = "VEOVEO_TEST_EXPLICIT_ASYNC_CLEANUP";
    let Ok(mode) = env::var(KEY) else {
        for mode in [
            "cancelled",
            "interrupted",
            "descendants",
            "expired",
            "cleared",
            "replaced",
        ] {
            crate::process::tests::isolated_control(
                "process::async_process::explicit_cleanup_tests::async_cleanup_keeps_original_owner_cap_and_failed_identity",
                KEY,
                mode,
            );
        }
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let original =
        crate::lifecycle::owner::test_scope(root.path().to_owned(), Duration::from_secs(1));
    crate::lifecycle::owner::test_activate(Some(&original));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let marker = root.path().join(".ready");
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                if mode == "descendants" {
                    "sleep 30 & printf ready > \"$MARKER\"; exit 0"
                } else {
                    "sleep 30 & printf ready > \"$MARKER\"; wait"
                },
            ])
            .env("MARKER", &marker);
        let mut child = crate::spawn_async(command).unwrap();
        let pid = child.guard.child.id();
        let registration = child.guard.registration.clone().unwrap();
        let ready_end = Instant::now() + Duration::from_secs(1);
        while !marker.exists() {
            assert!(Instant::now() < ready_end);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if mode == "descendants" {
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(
                child.guard.observe().is_err(),
                "live descendant permitted leader reap"
            );
        }
        crate::lifecycle::owner::test_cancel(&original);
        assert!(
            child.wait().await.is_err(),
            "ordinary wait accepted cancelled operation"
        );
        let other_root = tempfile::tempdir().unwrap();
        let other = crate::lifecycle::owner::test_scope(
            other_root.path().to_owned(),
            Duration::from_secs(5),
        );
        if mode == "cleared" {
            crate::lifecycle::owner::test_activate(None);
        }
        if mode == "replaced" {
            crate::lifecycle::owner::test_activate(Some(&other));
        }
        let deadline = if mode == "expired" {
            Instant::now() - Duration::from_millis(1)
        } else {
            Instant::now() + Duration::from_millis(500)
        };
        if mode == "interrupted" {
            use std::future::Future;
            let mut wait = Box::pin(child.cleanup_until(deadline));
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(wait.as_mut().poll(&mut context).is_pending());
            drop(wait);
            assert_eq!(
                child.guard.explicit_cleanup.as_ref().unwrap().end,
                deadline.min(original.cleanup_end())
            );
        }
        let result = child.cleanup_until(deadline).await;
        if matches!(mode.as_str(), "expired" | "cleared" | "replaced") {
            assert!(result.is_err());
            let cap = child.guard.explicit_cleanup.as_ref().unwrap().end;
            assert!(
                child
                    .cleanup_until(Instant::now() + Duration::from_secs(20))
                    .await
                    .is_err()
            );
            assert_eq!(child.guard.explicit_cleanup.as_ref().unwrap().end, cap);
            // Move the actual child out only after running its fallback, allowing
            // this fixture to reconcile a deliberately unresolved identity.
            let started = Instant::now();
            child.guard.stop();
            assert!(started.elapsed() < Duration::from_millis(100));
            assert!(
                registration.exists(),
                "failed cleanup lost original registration"
            );
            assert!(
                Path::new(&format!("/proc/{pid}")).exists(),
                "failed cleanup reaped identity"
            );
            if mode == "expired" {
                assert!(crate::lifecycle::live_group(pid).unwrap());
            }
            nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            )
            .unwrap();
            let end = Instant::now() + Duration::from_secs(1);
            while crate::lifecycle::live_group(pid).unwrap() {
                assert!(Instant::now() < end);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            // Fixture-only explicit reconciliation after proving failed-state retention.
            child.guard.explicit_cleanup = None;
            child.guard.observe().unwrap().unwrap();
        } else {
            result.unwrap();
            assert!(!registration.exists());
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            child
                .cleanup_until(Instant::now() + Duration::from_secs(20))
                .await
                .unwrap();
        }
        drop(child);
        crate::lifecycle::owner::test_activate(None);
    });
}
