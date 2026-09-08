use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub org: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub pat: String,
}

pub fn path() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
        })
        .join("azura/config.toml")
}

impl Config {
    /// Nunca falha: devolve o que achar, possivelmente incompleto.
    /// Ordem: arquivo → variáveis de ambiente (que sobrescrevem).
    pub fn load() -> Self {
        let mut cfg: Config = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();

        if let Ok(v) = std::env::var("AZDO_ORG") {
            cfg.org = v;
        }
        if let Ok(v) = std::env::var("AZDO_PROJECT") {
            cfg.project = v;
        }
        for var in ["AZDO_PAT", "AZURE_DEVOPS_EXT_PAT"] {
            if let Ok(v) = std::env::var(var)
                && !v.is_empty()
            {
                cfg.pat = v;
                break;
            }
        }
        cfg
    }

    pub fn is_complete(&self) -> bool {
        !self.org.is_empty() && !self.project.is_empty() && !self.pat.is_empty()
    }

    pub fn save(&self) -> Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, toml::to_string_pretty(self)?)
            .with_context(|| format!("não consegui escrever {}", p.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // guarda um token: só o dono lê
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Aproveita os defaults de quem já usa `az devops configure`.
    pub fn from_az() -> (String, String) {
        let p = PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join(".azure/azuredevops/config");
        let Ok(text) = std::fs::read_to_string(p) else {
            return (String::new(), String::new());
        };
        let mut org = String::new();
        let mut project = String::new();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "organization" => org = v.rsplit('/').next().unwrap_or(v).to_string(),
                "project" => project = v.to_string(),
                _ => {}
            }
        }
        (org, project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_defaults_do_az() {
        // formato real de ~/.azure/azuredevops/config
        let dir = std::env::temp_dir().join(format!("azura-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".azure/azuredevops")).unwrap();
        std::fs::write(
            dir.join(".azure/azuredevops/config"),
            "[defaults]\norganization = https://dev.azure.com/contoso\nproject = Fabrikam\n",
        )
        .unwrap();
        unsafe { std::env::set_var("HOME", &dir) };
        assert_eq!(Config::from_az(), ("contoso".into(), "Fabrikam".into()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
