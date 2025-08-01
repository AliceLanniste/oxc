#![expect(rustdoc::private_intra_doc_links)] // useful for intellisense

use std::{ops::Deref, path::Path, rc::Rc};

use javascript_globals::GLOBALS;

use oxc_ast::ast::IdentifierReference;
use oxc_cfg::ControlFlowGraph;
use oxc_diagnostics::{OxcDiagnostic, Severity};
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};

#[cfg(debug_assertions)]
use crate::rule::RuleFixMeta;
use crate::{
    AllowWarnDeny, FrameworkFlags, ModuleRecord, OxlintEnv, OxlintGlobals, OxlintSettings,
    config::GlobalValue,
    disable_directives::DisableDirectives,
    fixer::{Fix, FixKind, Message, PossibleFixes, RuleFix, RuleFixer},
};

mod host;
pub use host::ContextHost;

/// Contains all of the state and context specific to this lint rule.
///
/// Includes information like the rule name, plugin name, and severity of the rule.
/// It also has a reference to the shared linting data [`ContextHost`], which is the same for all rules.
#[derive(Clone)]
#[must_use]
pub struct LintContext<'a> {
    /// Shared context independent of the rule being linted.
    parent: Rc<ContextHost<'a>>,
    /// Name of the plugin this rule belongs to. Example: `eslint`, `unicorn`, `react`
    current_plugin_name: &'static str,
    /// Prefixed version of the plugin name. Examples:
    /// - `eslint-plugin-react`, for `react` plugin,
    /// - `typescript-eslint`, for `typescript` plugin,
    /// - `eslint-plugin-import`, for `import` plugin.
    current_plugin_prefix: &'static str,
    /// Kebab-cased name of the current rule being linted. Example: `no-unused-vars`, `no-undef`.
    current_rule_name: &'static str,
    /// Capabilities of the current rule to fix issues. Indicates whether:
    /// - Rule cannot be auto-fixed [`RuleFixMeta::None`]
    /// - Rule needs an auto-fix to be written still [`RuleFixMeta::FixPending`]
    /// - Rule can be fixed in some cases [`RuleFixMeta::Conditional`]
    /// - Rule is fully auto-fixable [`RuleFixMeta::Fixable`]
    #[cfg(debug_assertions)]
    current_rule_fix_capabilities: RuleFixMeta,
    /// Current rule severity. Allows for user severity overrides, e.g.
    /// ```json
    /// // .oxlintrc.json
    /// {
    ///   "rules": {
    ///     "no-debugger": "error"
    ///   }
    /// }
    /// ```
    severity: Severity,
}

impl<'a> Deref for LintContext<'a> {
    type Target = Semantic<'a>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.parent.semantic()
    }
}

impl<'a> LintContext<'a> {
    /// Base URL for the documentation, used to generate rule documentation URLs when a diagnostic is reported.
    const WEBSITE_BASE_URL: &'static str = "https://oxc.rs/docs/guide/usage/linter/rules";

    /// Set the plugin name for the current rule.
    pub fn with_plugin_name(mut self, plugin: &'static str) -> Self {
        self.current_plugin_name = plugin;
        self.current_plugin_prefix = plugin_name_to_prefix(plugin);
        self
    }

    /// Set the current rule name. Name should be kebab-cased like: `no-unused-vars` or `no-undef`.
    pub fn with_rule_name(mut self, name: &'static str) -> Self {
        self.current_rule_name = name;
        self
    }

    /// Set the current rule fix capabilities. See [`RuleFixMeta`] for more information.
    #[cfg(debug_assertions)]
    pub fn with_rule_fix_capabilities(mut self, capabilities: RuleFixMeta) -> Self {
        self.current_rule_fix_capabilities = capabilities;
        self
    }

    /// Update the severity of diagnostics reported by the rule this context is
    /// associated with.
    #[inline]
    pub fn with_severity(mut self, severity: AllowWarnDeny) -> Self {
        self.severity = Severity::from(severity);
        self
    }

    /// Get information such as the control flow graph, bound symbols, AST, etc.
    /// for the file being linted.
    ///
    /// Refer to [`Semantic`]'s documentation for more information.
    #[inline]
    pub fn semantic(&self) -> &Rc<Semantic<'a>> {
        &self.parent.semantic
    }

    #[inline]
    pub fn module_record(&self) -> &ModuleRecord {
        self.parent.module_record()
    }

    /// Get the control flow graph for the current program.
    #[inline]
    pub fn cfg(&self) -> &ControlFlowGraph {
        // SAFETY: `LintContext::new` is the only way to construct a `LintContext` and we always
        // assert the existence of control flow so it should always be `Some`.
        unsafe { self.parent.semantic.cfg().unwrap_unchecked() }
    }

    /// List of all disable directives in the file being linted.
    #[inline]
    pub fn disable_directives(&self) -> &DisableDirectives<'a> {
        &self.parent.disable_directives
    }

    /// Get a snippet of source text covered by the given [`Span`]. For details,
    /// see [`Span::source_text`].
    pub fn source_range(&self, span: Span) -> &'a str {
        span.source_text(self.parent.semantic.source_text())
    }

    /// Path to the file currently being linted.
    #[inline]
    pub fn file_path(&self) -> &Path {
        &self.parent.file_path
    }

    /// Plugin settings
    #[inline]
    pub fn settings(&self) -> &OxlintSettings {
        &self.parent.config.settings
    }

    /// Sets of global variables that have been enabled or disabled.
    #[inline]
    pub fn globals(&self) -> &OxlintGlobals {
        &self.parent.config.globals
    }

    /// Checks if the provided identifier is a reference to a global variable.
    pub fn is_reference_to_global_variable(&self, ident: &IdentifierReference) -> bool {
        let name = ident.name.as_str();
        self.scoping().root_unresolved_references().contains_key(name)
            && !self.globals().get(name).is_some_and(|value| *value == GlobalValue::Off)
    }

    /// Checks if the provided identifier is a reference to a global variable.
    pub fn get_global_variable_value(&self, name: &str) -> Option<GlobalValue> {
        if !self.scoping().root_unresolved_references().contains_key(name) {
            return None;
        }

        if let Some(value) = self.globals().get(name) {
            return Some(*value);
        }

        self.get_env_global_entry(name)
    }

    /// Runtime environments turned on/off by the user.
    ///
    /// Examples of environments are `builtin`, `browser`, `node`, etc.
    #[inline]
    pub fn env(&self) -> &OxlintEnv {
        &self.parent.config.env
    }

    fn get_env_global_entry(&self, var: &str) -> Option<GlobalValue> {
        // builtin is always readonly
        if GLOBALS["builtin"].contains_key(var) {
            return Some(GlobalValue::Readonly);
        }

        for env in self.env().iter() {
            if let Some(env) = GLOBALS.get(env) {
                if let Some(value) = env.get(var) {
                    return Some(GlobalValue::from(*value));
                }
            }
        }

        None
    }

    /// Checks if a given variable named is defined as a global variable in the current environment.
    ///
    /// Example:
    /// - `env_contains_var("Date")` returns `true` because it is a global builtin in all environments.
    /// - `env_contains_var("HTMLElement")` returns `true` only if the `browser` environment is enabled.
    /// - `env_contains_var("globalThis")` returns `true` only if the `es2020` environment or higher is enabled.
    pub fn env_contains_var(&self, var: &str) -> bool {
        if GLOBALS["builtin"].contains_key(var) {
            return true;
        }
        for env in self.env().iter() {
            if let Some(env) = GLOBALS.get(env) {
                if env.contains_key(var) {
                    return true;
                }
            }
        }
        false
    }

    /* Diagnostics */

    /// Add a diagnostic message to the list of diagnostics. Outputs a diagnostic with the current rule
    /// name, severity, and a link to the rule's documentation URL.
    fn add_diagnostic(&self, mut message: Message<'a>) {
        if self.parent.disable_directives.contains(self.current_rule_name, message.span()) {
            return;
        }
        message.error = message
            .error
            .with_error_code(self.current_plugin_prefix, self.current_rule_name)
            .with_url(format!(
                "{}/{}/{}.html",
                Self::WEBSITE_BASE_URL,
                self.current_plugin_name,
                self.current_rule_name
            ));
        if message.error.severity != self.severity {
            message.error = message.error.with_severity(self.severity);
        }

        self.parent.push_diagnostic(message);
    }

    /// Report a lint rule violation.
    ///
    /// Use [`LintContext::diagnostic_with_fix`] to provide an automatic fix.
    #[inline]
    pub fn diagnostic(&self, diagnostic: OxcDiagnostic) {
        self.add_diagnostic(Message::new(diagnostic, PossibleFixes::None));
    }

    /// Report a lint rule violation and provide an automatic fix.
    ///
    /// The second argument is a [closure] that takes a [`RuleFixer`] and
    /// returns something that can turn into a `CompositeFix`.
    ///
    /// Fixes created this way should not create parse errors or change the
    /// semantics of the linted code. If your fix may change the code's
    /// semantics, use [`LintContext::diagnostic_with_suggestion`] instead. If
    /// your fix has the potential to create parse errors, use
    /// [`LintContext::diagnostic_with_dangerous_fix`].
    ///
    /// [closure]: <https://doc.rust-lang.org/book/ch13-01-closures.html>
    #[inline]
    pub fn diagnostic_with_fix<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
    where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        self.diagnostic_with_fix_of_kind(diagnostic, FixKind::SafeFix, fix);
    }

    /// Report a lint rule violation and provide a suggestion for fixing it.
    ///
    /// The second argument is a [closure] that takes a [`RuleFixer`] and
    /// returns something that can turn into a `CompositeFix`.
    ///
    /// Fixes created this way should not create parse errors, but have the
    /// potential to change the code's semantics. If your fix is completely safe
    /// and definitely does not change semantics, use [`LintContext::diagnostic_with_fix`].
    /// If your fix has the potential to create parse errors, use
    /// [`LintContext::diagnostic_with_dangerous_fix`].
    ///
    /// [closure]: <https://doc.rust-lang.org/book/ch13-01-closures.html>
    #[inline]
    pub fn diagnostic_with_suggestion<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
    where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        self.diagnostic_with_fix_of_kind(diagnostic, FixKind::Suggestion, fix);
    }

    /// Report a lint rule violation and provide a suggestion for fixing it.
    ///
    /// The second argument is a [closure] that takes a [`RuleFixer`] and
    /// returns something that can turn into a `CompositeFix`.
    ///
    /// Fixes created this way should not create parse errors, but have the
    /// potential to change the code's semantics. If your fix is completely safe
    /// and definitely does not change semantics, use [`LintContext::diagnostic_with_fix`].
    /// If your fix has the potential to create parse errors, use
    /// [`LintContext::diagnostic_with_dangerous_fix`].
    ///
    /// [closure]: <https://doc.rust-lang.org/book/ch13-01-closures.html>
    #[inline]
    pub fn diagnostic_with_dangerous_suggestion<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
    where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        self.diagnostic_with_fix_of_kind(diagnostic, FixKind::DangerousSuggestion, fix);
    }

    /// Report a lint rule violation and provide a potentially dangerous
    /// automatic fix for it.
    ///
    /// The second argument is a [closure] that takes a [`RuleFixer`] and
    /// returns something that can turn into a `CompositeFix`.
    ///
    /// Dangerous fixes should be avoided and are not applied by default with
    /// `--fix`. Use this method if:
    /// - Your fix is experimental and you want to test it out in the wild
    ///   before marking it as safe.
    /// - Your fix is extremely aggressive and risky, but you want to provide
    ///   it as an option to users.
    ///
    /// When possible, prefer [`LintContext::diagnostic_with_fix`]. If the only
    /// risk your fix poses is minor(ish) changes to code semantics, use
    /// [`LintContext::diagnostic_with_suggestion`] instead.
    ///
    /// [closure]: <https://doc.rust-lang.org/book/ch13-01-closures.html>
    ///
    #[inline]
    pub fn diagnostic_with_dangerous_fix<C, F>(&self, diagnostic: OxcDiagnostic, fix: F)
    where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        self.diagnostic_with_fix_of_kind(diagnostic, FixKind::DangerousFix, fix);
    }

    /// Report a lint rule violation and provide an automatic fix of a specific kind.
    ///
    /// The second argument is a [closure] that takes a [`RuleFixer`] and
    /// returns something that can turn into a [`RuleFix`].
    ///
    /// [closure]: <https://doc.rust-lang.org/book/ch13-01-closures.html>
    pub fn diagnostic_with_fix_of_kind<C, F>(
        &self,
        diagnostic: OxcDiagnostic,
        fix_kind: FixKind,
        fix: F,
    ) where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        let (diagnostic, fix) = self.create_fix(fix_kind, fix, diagnostic);
        if let Some(fix) = fix {
            self.add_diagnostic(Message::new(diagnostic, PossibleFixes::Single(fix)));
        } else {
            self.diagnostic(diagnostic);
        }
    }

    /// Report a lint rule violation and provide an automatic fix of a specific kind.
    /// This method is used when the rule can provide multiple fixes for the same diagnostic.
    pub fn diagnostics_with_multiple_fixes<C, F1, F2>(
        &self,
        diagnostic: OxcDiagnostic,
        fix_one: (FixKind, F1),
        fix_two: (FixKind, F2),
    ) where
        C: Into<RuleFix<'a>>,
        F1: FnOnce(RuleFixer<'_, 'a>) -> C,
        F2: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        let fixes_result: Vec<Fix<'a>> = vec![
            self.create_fix(fix_one.0, fix_one.1, diagnostic.clone()).1,
            self.create_fix(fix_two.0, fix_two.1, diagnostic.clone()).1,
        ]
        .into_iter()
        .flatten()
        .collect();

        if fixes_result.is_empty() {
            self.diagnostic(diagnostic);
        } else {
            self.add_diagnostic(Message::new(diagnostic, PossibleFixes::Multiple(fixes_result)));
        }
    }

    fn create_fix<C, F>(
        &self,
        fix_kind: FixKind,
        fix: F,
        diagnostic: OxcDiagnostic,
    ) -> (OxcDiagnostic, Option<Fix<'a>>)
    where
        C: Into<RuleFix<'a>>,
        F: FnOnce(RuleFixer<'_, 'a>) -> C,
    {
        let fixer = RuleFixer::new(fix_kind, self);
        let rule_fix: RuleFix<'a> = fix(fixer).into();
        #[cfg(debug_assertions)]
        debug_assert!(
            self.current_rule_fix_capabilities.supports_fix(fix_kind),
            "Rule `{}` does not support safe fixes. Did you forget to update fix capabilities in declare_oxc_lint?.\n\tSupported fix kinds: {:?}\n\tAttempted fix kind: {:?}",
            self.current_rule_name,
            FixKind::from(self.current_rule_fix_capabilities),
            rule_fix.kind()
        );

        let diagnostic = match (rule_fix.message(), &diagnostic.help) {
            (Some(message), None) => diagnostic.with_help(message.to_owned()),
            _ => diagnostic,
        };

        if self.parent.fix.can_apply(rule_fix.kind()) && !rule_fix.is_empty() {
            let fix = rule_fix.into_fix(self.source_text());
            #[cfg(debug_assertions)]
            {
                if fix.span.size() > 1 {
                    debug_assert!(
                        fix.message.as_ref().is_some_and(|msg| !msg.is_empty()),
                        "Rule `{}/{}` fix should have a message for a complex fix. Did you forget to add a message?\n   Source text: {:?}\n    Fixed text: {:?}\nhelp: You can add a message to a fix with `RuleFix.with_message()`",
                        self.current_plugin_name,
                        self.current_rule_name,
                        self.source_range(fix.span),
                        fix.content
                    );
                }
            }

            (diagnostic, Some(fix))
        } else {
            (diagnostic, None)
        }
    }

    /// Framework flags, indicating front-end frameworks that might be in use.
    pub fn frameworks(&self) -> FrameworkFlags {
        self.parent.frameworks
    }
}

/// Gets the prefixed plugin name, given the short plugin name.
///
/// Example:
///
/// ```text
/// assert_eq!(plugin_name_to_prefix("react"), "eslint-plugin-react");
/// ```
#[inline]
fn plugin_name_to_prefix(plugin_name: &'static str) -> &'static str {
    PLUGIN_PREFIXES.get(plugin_name).copied().unwrap_or(plugin_name)
}

/// Map of plugin names to their prefixed versions.
const PLUGIN_PREFIXES: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "import" => "eslint-plugin-import",
    "jest" => "eslint-plugin-jest",
    "jsdoc" => "eslint-plugin-jsdoc",
    "jsx_a11y" => "eslint-plugin-jsx-a11y",
    "nextjs" => "eslint-plugin-next",
    "promise" => "eslint-plugin-promise",
    "react_perf" => "eslint-plugin-react-perf",
    "react" => "eslint-plugin-react",
    "typescript" => "typescript-eslint",
    "unicorn" => "eslint-plugin-unicorn",
    "vitest" => "eslint-plugin-vitest",
    "node" => "eslint-plugin-node",
};
