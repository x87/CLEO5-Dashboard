pub struct Config {
    pub github_owner: &'static str,
    pub github_repo: &'static str,
    pub github_api_base: &'static str,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            github_owner: "cleolibrary",
            github_repo: "CLEO5",
            github_api_base: "https://api.github.com",
        }
    }
}

impl Config {
    pub fn get_github_releases_url(&self) -> String {
        format!(
            "{}/repos/{}/{}/releases/latest",
            self.github_api_base, self.github_owner, self.github_repo
        )
    }
}
