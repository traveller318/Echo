/*!
 * SOURCE OF TRUTH KEYWORDS: system commands, appearance_get, AppearanceView, theme, transparency, backdrop, app-wide commands
 * WHAT:  The system command group (02 §4.3). `appearance_get` returns the AppearanceView (theme, transparency,
 *        backdrop) both windows paint from.
 * WHY:   The UI reads appearance once through this command and then stays fresh from AppearanceChanged, never
 *        polling (02 §4.4); the view is computed on demand from the live settings and the SystemAppearance port,
 *        so there is no cached copy to drift. The logs-folder, privacy-settings and update commands join this
 *        group with their steps.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.appearanceGet()` by
 *        src/lib/appearance.ts.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::appearance,
    types::{AppError, AppearanceView},
};

echo_command! {
    /// The theme, transparency and backdrop both windows paint from right now.
    name: appearance_get,
    output: AppearanceView,
    permission: None,
    reentrancy: Shared,
    handler: get_appearance,
}

/// The appearance view for the settings in effect and the current Windows switches.
pub async fn get_appearance(ctx: &CommandCtx, (): ()) -> Result<AppearanceView, AppError> {
    Ok(appearance::current(&ctx.settings(), ctx.appearance()))
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::{FakePrivacyConsent, poll_once},
        registry,
        types::{Backdrop, CommandSpec, Reentrancy, ThemePreference, Transparency},
    };

    const SPEC: CommandSpec = CommandSpec {
        name: "appearance_get",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    fn run(ctx: &CommandCtx) -> AppearanceView {
        let Poll::Ready(Ok(view)) = poll_once(factory::run(ctx, &SPEC, (), get_appearance)) else {
            panic!("appearance_get did not return a view");
        };
        view
    }

    #[test]
    fn returns_the_live_view_and_follows_the_transparency_switch() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        assert_eq!(
            run(&harness.ctx),
            AppearanceView {
                theme: ThemePreference::System,
                transparency: Transparency::Full,
                backdrop: Backdrop::Mica,
            }
        );
        harness.appearance.switch(Transparency::Reduced);
        let reduced = run(&harness.ctx);
        assert_eq!(reduced.transparency, Transparency::Reduced);
        assert_eq!(reduced.backdrop, Backdrop::Solid);
    }
}
