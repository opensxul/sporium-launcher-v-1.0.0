use super::model::*;
use crate::error::CoreError;

pub trait ContentProvider: Send + Sync {
    fn version_from_hash(&self, _sha512: &str) -> Result<Option<ContentVersion>, CoreError> {
        Ok(None)
    }
    fn search(&self, query: &CatalogQuery) -> Result<CatalogPage, CoreError>;
    fn project(&self, id: &str) -> Result<ContentProject, CoreError>;
    fn versions(&self, id: &str, minecraft: &str) -> Result<Vec<ContentVersion>, CoreError>;
    fn version(&self, id: &str) -> Result<ContentVersion, CoreError>;
    fn tags(&self) -> Result<ContentTags, CoreError>;
    /// Provider-side preparation for server profiles; never a client installation path.
    fn plugin_versions(
        &self,
        id: &str,
        minecraft: &str,
        platform: &str,
    ) -> Result<Vec<ContentVersion>, CoreError> {
        if minecraft.is_empty() || !matches!(platform, "paper" | "purpur") {
            return Err(CoreError::InvalidInput);
        }
        let project = self.project(id)?;
        let mut versions = self.versions(id, minecraft)?;
        versions.retain(|v| super::resolve::plugin_compatible(&project, v, minecraft, platform));
        versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
        Ok(versions)
    }
}
