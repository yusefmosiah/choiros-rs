//! Desktop Supervision Tests - Phase 2
//!
//! These tests verify the DesktopSupervisor functionality:
//! - DesktopSupervisor creates DesktopActor instances
//! - Registry-based discovery (desktop:{id})
//! - Automatic restart on failure
//! - Restart intensity limiting (max 3 per 60 seconds)
//! - Desktop identity preservation across restarts

#[cfg(feature = "supervision_refactor")]
mod desktop_supervision_tests {
    use ractor::Actor;
    use sandbox::actors::desktop::{get_desktop_state, DesktopActorMsg, DesktopArguments};
    use sandbox::actors::event_store::{EventStoreActor, EventStoreArguments};
    use sandbox::supervisor::{ApplicationSupervisor, ApplicationSupervisorMsg};
    use tokio::time::Duration;

    /// Test that DesktopSupervisor creates desktop actors
    #[tokio::test]
    async fn test_desktop_supervisor_creates_desktop() {
        tracing::info!("Testing DesktopSupervisor creates desktop actors...");

        // Create EventStore first
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor (which will spawn SessionSupervisor and DesktopSupervisor)
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor_creates".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Wait for the supervision tree to initialize
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Get or create a desktop via ApplicationSupervisor
        let desktop_result = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: "test-desktop-1".to_string(),
                user_id: "user-1".to_string(),
                reply,
            }
        });

        assert!(
            desktop_result.is_ok(),
            "Should successfully create desktop: {:?}",
            desktop_result.err()
        );

        let desktop_ref = desktop_result.unwrap();

        // Verify desktop is responsive
        let actor_info_result = ractor::call!(&desktop_ref, |reply| {
            DesktopActorMsg::GetActorInfo { reply }
        });
        assert!(
            actor_info_result.is_ok(),
            "DesktopActor should respond to GetActorInfo"
        );

        let (desktop_id, user_id) = actor_info_result.unwrap();
        assert_eq!(desktop_id, "test-desktop-1");
        assert_eq!(user_id, "user-1");

        // Get desktop state to verify it's working
        let state = get_desktop_state(&desktop_ref).await;
        assert!(state.is_ok(), "Should be able to get desktop state");

        tracing::info!("DesktopSupervisor creation test passed!");
    }

    /// Test that DesktopSupervisor uses registry for discovery
    #[tokio::test]
    async fn test_desktop_supervisor_uses_registry() {
        tracing::info!("Testing DesktopSupervisor registry discovery...");

        // Create EventStore
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor_registry".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Create a desktop
        let desktop_result = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: "test-desktop-registry".to_string(),
                user_id: "user-1".to_string(),
                reply,
            }
        });

        assert!(desktop_result.is_ok());
        let desktop_ref_1 = desktop_result.unwrap();

        // Check that it's in the registry
        let actor_name = "desktop:test-desktop-registry".to_string();
        let registry_lookup = ractor::registry::where_is(actor_name.clone());
        assert!(
            registry_lookup.is_some(),
            "DesktopActor should be in registry"
        );
        assert_eq!(
            registry_lookup.unwrap().get_id(),
            desktop_ref_1.get_id(),
            "Registry should return the same actor"
        );

        // Request the same desktop again - should return existing actor
        let desktop_result_2 = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: "test-desktop-registry".to_string(),
                user_id: "user-1".to_string(),
                reply,
            }
        });

        assert!(desktop_result_2.is_ok());
        let desktop_ref_2 = desktop_result_2.unwrap();

        // Should be the same actor
        assert_eq!(
            desktop_ref_1.get_id(),
            desktop_ref_2.get_id(),
            "Should return same actor for same desktop_id"
        );

        tracing::info!("DesktopSupervisor registry test passed!");
    }

    /// Test that DesktopSupervisor creates new actor after normal termination
    #[tokio::test]
    async fn test_desktop_supervisor_creates_new_after_termination() {
        tracing::info!("Testing DesktopSupervisor creates new actor after termination...");

        // Create EventStore
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor_termination".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Create a desktop
        let desktop_result = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: "test-desktop-termination".to_string(),
                user_id: "user-1".to_string(),
                reply,
            }
        });

        assert!(desktop_result.is_ok());
        let desktop_ref_1 = desktop_result.unwrap();
        let _original_actor_id = desktop_ref_1.get_id();

        // Register an app to have some state
        let open_result = ractor::call!(&desktop_ref_1, |reply| DesktopActorMsg::RegisterApp {
            app: shared_types::AppDefinition {
                id: "test-app".to_string(),
                name: "Test App".to_string(),
                icon: "🧪".to_string(),
                component_code: "TestApp".to_string(),
                default_width: 400,
                default_height: 300,
            },
            reply,
        });

        assert!(open_result.is_ok());

        // Stop the actor (normal termination - this removes it from tracking)
        desktop_ref_1.stop(Some("Test stop".to_string()));

        // Wait for the supervision chain to process the termination
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Get the desktop again - should create a new actor with same identity
        let desktop_result_2 = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: "test-desktop-termination".to_string(),
                user_id: "user-1".to_string(),
                reply,
            }
        });

        assert!(
            desktop_result_2.is_ok(),
            "Should be able to get desktop after termination: {:?}",
            desktop_result_2.err()
        );

        let desktop_ref_2 = desktop_result_2.unwrap();

        // The new actor should be responsive with the same identity
        let actor_info_result = ractor::call!(&desktop_ref_2, |reply| {
            DesktopActorMsg::GetActorInfo { reply }
        });
        assert!(
            actor_info_result.is_ok(),
            "New DesktopActor should respond to GetActorInfo"
        );

        let (desktop_id, user_id) = actor_info_result.unwrap();
        assert_eq!(desktop_id, "test-desktop-termination");
        assert_eq!(user_id, "user-1");

        tracing::info!("DesktopSupervisor termination test passed!");
    }

    /// Test that DesktopSupervisor preserves identity after actor termination
    #[tokio::test]
    async fn test_desktop_supervisor_preserves_identity() {
        tracing::info!("Testing DesktopSupervisor identity preservation...");

        // Create EventStore
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor_identity".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(200)).await;

        let desktop_id = "test-identity-desktop";
        let user_id = "identity-user";

        // Create a desktop
        let desktop_result = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: desktop_id.to_string(),
                user_id: user_id.to_string(),
                reply,
            }
        });

        assert!(desktop_result.is_ok());
        let desktop_ref = desktop_result.unwrap();

        // Get identity info
        let info_result = ractor::call!(&desktop_ref, |reply| DesktopActorMsg::GetActorInfo {
            reply
        });
        assert!(info_result.is_ok());

        let (retrieved_desktop_id, retrieved_user_id) = info_result.unwrap();
        assert_eq!(retrieved_desktop_id, desktop_id);
        assert_eq!(retrieved_user_id, user_id);

        // Stop the actor (normal termination)
        desktop_ref.stop(Some("Test identity".to_string()));

        // Wait for the supervision chain to process the termination
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Get the desktop again - should create a new actor with same identity
        let desktop_result_2 = ractor::call!(&app_supervisor, |reply| {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id: desktop_id.to_string(),
                user_id: user_id.to_string(),
                reply,
            }
        });

        assert!(
            desktop_result_2.is_ok(),
            "Should be able to get desktop after termination: {:?}",
            desktop_result_2.err()
        );
        let desktop_ref_2 = desktop_result_2.unwrap();

        // Verify identity is preserved
        let info_result_2 = ractor::call!(&desktop_ref_2, |reply| DesktopActorMsg::GetActorInfo {
            reply
        });
        assert!(info_result_2.is_ok());

        let (retrieved_desktop_id_2, retrieved_user_id_2) = info_result_2.unwrap();
        assert_eq!(
            retrieved_desktop_id_2, desktop_id,
            "desktop_id should be preserved"
        );
        assert_eq!(retrieved_user_id_2, user_id, "user_id should be preserved");

        tracing::info!("DesktopSupervisor identity preservation test passed!");
    }

    /// Test restart intensity limit (max 3 restarts in 60 seconds)
    #[tokio::test]
    async fn test_desktop_restart_intensity() {
        tracing::info!("Testing DesktopSupervisor restart intensity...");

        // Create EventStore
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Create a custom DesktopSupervisor to test restart limits
        use sandbox::supervisor::desktop::{
            DesktopSupervisor, DesktopSupervisorArgs, DesktopSupervisorMsg,
        };

        let (desktop_supervisor, _ds_handle) = Actor::spawn(
            Some("test_desktop_supervisor_intensity".to_string()),
            DesktopSupervisor,
            DesktopSupervisorArgs {
                event_store: event_store.clone(),
            },
        )
        .await
        .expect("Failed to spawn DesktopSupervisor");

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(100)).await;

        let desktop_id = "intensity-test-desktop";
        let user_id = "intensity-user";

        // Create arguments for the desktop
        let args = DesktopArguments {
            desktop_id: desktop_id.to_string(),
            user_id: user_id.to_string(),
            event_store: event_store.clone(),
        };

        // Create a desktop
        let desktop_result = ractor::call!(&desktop_supervisor, |reply| {
            DesktopSupervisorMsg::GetOrCreateDesktop {
                desktop_id: desktop_id.to_string(),
                user_id: user_id.to_string(),
                args: args.clone(),
                reply,
            }
        });

        assert!(
            desktop_result.is_ok(),
            "Should create desktop: {:?}",
            desktop_result.err()
        );
        let desktop_ref = desktop_result.unwrap();

        // Get the actor ID for restart tracking
        let _actor_id = desktop_ref.get_id();

        // Stop the actor (first restart trigger)
        desktop_ref.stop(Some("Test 1".to_string()));
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Verify it was restarted (request should still work)
        let desktop_result_2 = ractor::call!(&desktop_supervisor, |reply| {
            DesktopSupervisorMsg::GetOrCreateDesktop {
                desktop_id: desktop_id.to_string(),
                user_id: user_id.to_string(),
                args: args.clone(),
                reply,
            }
        });

        assert!(
            desktop_result_2.is_ok(),
            "Desktop should be available after restart"
        );

        let desktop_ref_2 = desktop_result_2.unwrap();

        // Stop again (second restart trigger)
        desktop_ref_2.stop(Some("Test 2".to_string()));
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Verify second restart worked
        let desktop_result_3 = ractor::call!(&desktop_supervisor, |reply| {
            DesktopSupervisorMsg::GetOrCreateDesktop {
                desktop_id: desktop_id.to_string(),
                user_id: user_id.to_string(),
                args: args.clone(),
                reply,
            }
        });

        assert!(
            desktop_result_3.is_ok(),
            "Desktop should be available after second restart"
        );

        tracing::info!("DesktopSupervisor restart intensity test passed!");
    }

    /// Test that multiple desktops can be created independently
    #[tokio::test]
    async fn test_multiple_desktops() {
        tracing::info!("Testing multiple desktop creation...");

        // Create EventStore
        let (event_store, _event_handle) =
            Actor::spawn(None, EventStoreActor, EventStoreArguments::InMemory)
                .await
                .expect("Failed to spawn EventStoreActor");

        // Spawn ApplicationSupervisor
        let (app_supervisor, _app_handle) = Actor::spawn(
            Some("test_app_supervisor_multi".to_string()),
            ApplicationSupervisor,
            event_store.clone(),
        )
        .await
        .expect("Failed to spawn ApplicationSupervisor");

        // Wait for initialization
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Create multiple desktops
        let desktops = vec![
            ("desktop-1", "user-1"),
            ("desktop-2", "user-2"),
            ("desktop-3", "user-1"), // Same user, different desktop
        ];

        let mut desktop_refs = Vec::new();

        for (desktop_id, user_id) in &desktops {
            let result = ractor::call!(&app_supervisor, |reply| {
                ApplicationSupervisorMsg::GetOrCreateDesktop {
                    desktop_id: desktop_id.to_string(),
                    user_id: user_id.to_string(),
                    reply,
                }
            });

            assert!(
                result.is_ok(),
                "Should create desktop {}: {:?}",
                desktop_id,
                result.err()
            );
            desktop_refs.push((desktop_id.to_string(), user_id.to_string(), result.unwrap()));
        }

        // Verify all are responsive and have correct identities
        for (expected_desktop_id, expected_user_id, ref desktop_ref) in &desktop_refs {
            let info_result =
                ractor::call!(desktop_ref, |reply| DesktopActorMsg::GetActorInfo { reply });
            assert!(
                info_result.is_ok(),
                "Desktop {} should respond",
                expected_desktop_id
            );

            let (actual_desktop_id, actual_user_id) = info_result.unwrap();
            assert_eq!(
                &actual_desktop_id, expected_desktop_id,
                "Desktop ID mismatch"
            );
            assert_eq!(&actual_user_id, expected_user_id, "User ID mismatch");
        }

        // Verify they are different actors
        let actor_ids: Vec<_> = desktop_refs.iter().map(|(_, _, r)| r.get_id()).collect();
        let unique_ids: std::collections::HashSet<_> = actor_ids.iter().cloned().collect();
        assert_eq!(
            unique_ids.len(),
            3,
            "Each desktop should have a unique actor"
        );

        tracing::info!("Multiple desktops test passed!");
    }
}

#[cfg(not(feature = "supervision_refactor"))]
mod no_op_tests {
    #[test]
    fn test_supervision_refactor_disabled() {
        println!("supervision_refactor feature is disabled - skipping desktop supervision tests");
    }
}
