//! The two first-party browser applications share edge mechanics, not authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BrowserApp {
    #[default]
    Console,
    Workspace,
}

impl BrowserApp {
    pub(crate) const fn root(self) -> &'static str {
        match self {
            Self::Console => "/console/",
            Self::Workspace => "/workspace/",
        }
    }
    pub(crate) const fn api_root(self) -> &'static str {
        match self {
            Self::Console => "/console/api",
            Self::Workspace => "/workspace/api",
        }
    }

    pub(crate) const fn login(self) -> &'static str {
        match self {
            Self::Console => "/auth/login",
            Self::Workspace => "/workspace/auth/login",
        }
    }
    pub(crate) const fn callback(self) -> &'static str {
        match self {
            Self::Console => "/auth/callback",
            Self::Workspace => "/workspace/auth/callback",
        }
    }
    pub(crate) const fn session_cookie(self) -> &'static str {
        match self {
            Self::Console => "veoveo_console",
            Self::Workspace => "veoveo_workspace",
        }
    }
    pub(crate) const fn authorization_cookie(self) -> &'static str {
        match self {
            Self::Console => "veoveo_console_authorization",
            Self::Workspace => "veoveo_workspace_authorization",
        }
    }
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::Workspace => "Workspace",
        }
    }
}
