// Every command the window can invoke, in ONE list (ADR 0052, charter-app#276).
//
// This file is read twice. `lib.rs` hands the list to `tauri-specta`, which registers each
// command in the invoke handler and writes it into the TypeScript the UI imports. `build.rs`
// `include!`s the same file and hands the same names to Tauri's app manifest, which makes each
// one an `allow-<command>` permission and folds them into the two permission sets that the
// capabilities in `capabilities/` grant. So a command added here is registered, typed and allowed
// in one edit. A command registered anywhere else cannot be reached from the window at all:
// once an app has a manifest, Tauri refuses every app command that no capability grants.
//
// The two classes are what a review of this list is for:
//
// - `value_free`: nothing it returns is a secret's value. Most of the app.
// - `vault_values`: what puts a secret's value where the window can reach it, which is a vault's
//   reveal and its copy. A capability of their own grants them, to the main window only, so a
//   window added to the default capability later does not inherit them. A new command goes
//   here if it hands the window a value that the vault design (ADR 0047) keeps out of it. The
//   default is the other list, and adding one here means amending ADR 0052.
//
// Plain macros and no items, so the file means the same thing inside a build script as inside
// the crate. Paths are written `module::command`; the IPC knows a command by the last segment.

/// Hands the list, by class, to `$then!`.
macro_rules! app_commands {
    ($then:ident) => {
        $then! {
            value_free: [
                first_frame,
                title_bar_room,
                plane_at_launch,
                open_planes,
                close_plane,
                opener::recent_planes,
                opener::pick_project,
                opener::open_plane,
                opener::approve_plane,
                opener::planes_to_restore,
                opener::relaunch_ask,
                opener::relaunch,
                opener::window_holds_planes,
                windows::move_projects,
                windows::projects_handed,
                windows::charter_windows,
                windows::show_window_holding,
                opener::create_project,
                open_session,
                close_session,
                ignore_needs_you,
                send_input,
                resize_session,
                watch_session,
                unwatch_session,
                running_sessions,
                chat_states,
                opened_chats,
                chats_that_would_not_start,
                chats_plane_updated,
                chat_in_front,
                plane_pins,
                pin_project,
                pin_workspace,
                arrange_workspace_pins,
                pin_chat,
                chat_order,
                rename_chat,
                ask_to_quit,
                quit,
                quit_cancelled,
                hide_window,
                window_showing,
                plane_sidebar,
                workspace_panels,
                autosave::plane_fetch,
                saving::plane_saving,
                saving::choose_plane_mode,
                saving::save_plane,
                saving::workspace_saving,
                saving::save_repo,
                workspaces::workspace_create,
                live::workspace_live_preview,
                live::workspace_live,
                workspaces::workspace_at_risk,
                workspaces::workspace_remove,
                workspaces::workspace_rename,
                workspaces::workspace_starts_fresh,
                workspaces::workspace_focused,
                workspaces::reachable_repos,
                workspaces::take_repos,
                workspaces::clone_repo,
                workspaces::drop_repo,
                workspace_repos,
                alerts_everywhere,
                start_options,
                approve_profile,
                start_chat,
                worktrees::worktree_of_chat,
                worktrees::worktree_list,
                worktrees::worktree_remove,
                worktrees::worktree_merge,
                worktrees::worktree_done,
                updates::update_channel,
                updates::set_update_channel,
                updates::check_for_update,
                updates::install_update,
                updates::restart_to_update,
                extensions::installed_extensions,
                extensions::pick_extension,
                extensions::install_extension,
                extensions::approve_extension,
                extensions::forget_extension,
                extensions::set_extension_on,
                extensions::extension_themes,
                extensions::extension_panels,
                vaults::vault_list,
                vaults::vault_open,
                vaults::vault_refresh,
                vaults::vault_create,
                vaults::vault_remove,
                vaults::vault_secret_add,
                vaults::vault_secret_set,
                vaults::vault_secret_rename,
                vaults::vault_secret_delete,
                vaults::vault_identity_move,
                vaults::vault_identity_put,
                personas::persona_create,
                personas::persona_remove,
                personas::persona_edit,
                todos::todo_add,
                todos::todo_done,
                todos::todo_forget,
                extensions::project_extensions,
                extensions::extensions_on,
                extensions::extension_facts,
                harness_plugins::project_harness_plugins,
                extensions::project_theme,
                extensions::project_theme_drawn,
                views::extension_views,
                views::extension_programs_run,
                views::open_view,
                views::run_action,
                views::extension_commands,
                views::reopened_views,
                views::window_views,
                doctor::plane_doctor,
                settings::project_settings,
                settings::save_project_settings,
                settings::project_saving_in_force,
                settings::workspace_settings,
                settings::save_workspace_settings,
                usage::chat_usage,
                pin::plane_pin,
                about::about_charter,
                clipath::install_cli_on_path,
                windowprefs::write_layout,
                windowprefs::adopt_layout,
            ],
            vault_values: [
                vaults::vault_secret_reveal,
                vaults::vault_secret_copy,
            ],
        }
    };
}

/// The name the IPC knows a command by: the last segment of its path.
#[allow(
    unused_macros,
    reason = "used by build.rs and by the crate's tests, not by the app"
)]
macro_rules! command_name {
    ($only:ident) => {
        stringify!($only)
    };
    ($first:ident :: $($rest:ident)::+) => {
        command_name!($($rest)::+)
    };
}

/// The list's names, as `(value_free, vault_values)`: `app_commands!(command_names)`.
#[allow(
    unused_macros,
    reason = "used by build.rs and by the crate's tests, not by the app"
)]
macro_rules! command_names {
    (
        value_free: [$($($free:ident)::+),* $(,)?],
        vault_values: [$($($value:ident)::+),* $(,)?] $(,)?
    ) => {
        (
            &[$(command_name!($($free)::+)),*] as &'static [&'static str],
            &[$(command_name!($($value)::+)),*] as &'static [&'static str],
        )
    };
}
