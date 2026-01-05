//! Abstract class for app.

use std::path::PathBuf;

/// Trait for application implementations
pub trait BaseApp: Send + Sync {
    /// Unique name used for creating engine and widget
    fn app_name(&self) -> &str;
    
    /// App module string used in import
    fn app_module(&self) -> &str;
    
    /// Absolute path of app folder
    fn app_path(&self) -> PathBuf;
    
    /// Name for display on the menu
    fn display_name(&self) -> &str;
    
    /// Class name of app widget
    fn widget_name(&self) -> &str;
    
    /// Icon file name of app widget
    fn icon_name(&self) -> &str;
}

/// App information structure for registration
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub app_name: String,
    pub app_module: String,
    pub app_path: PathBuf,
    pub display_name: String,
    pub widget_name: String,
    pub icon_name: String,
}

impl AppInfo {
    /// Create new AppInfo
    pub fn new(
        app_name: impl Into<String>,
        app_module: impl Into<String>,
        app_path: PathBuf,
        display_name: impl Into<String>,
        widget_name: impl Into<String>,
        icon_name: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            app_module: app_module.into(),
            app_path,
            display_name: display_name.into(),
            widget_name: widget_name.into(),
            icon_name: icon_name.into(),
        }
    }
}

impl BaseApp for AppInfo {
    fn app_name(&self) -> &str {
        &self.app_name
    }

    fn app_module(&self) -> &str {
        &self.app_module
    }

    fn app_path(&self) -> PathBuf {
        self.app_path.clone()
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn widget_name(&self) -> &str {
        &self.widget_name
    }

    fn icon_name(&self) -> &str {
        &self.icon_name
    }
}
