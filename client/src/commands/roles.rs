use kore_protocol::api::{CreateRoleRequest, CreateSkillRequest, RoleSummary, SkillSummary};

use crate::config;

#[derive(clap::Subcommand, Debug)]
pub enum RoleCmd {
    /// Create a behavioral role in a project
    Create {
        title: String,
        /// Role body text
        #[arg(long)]
        content: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// List roles in a project
    List {
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum SkillCmd {
    /// Create a skill in a project
    Create {
        name: String,
        /// Skill body (or load command with --kind command)
        #[arg(long)]
        content: String,
        #[arg(long, default_value = "")]
        description: String,
        /// command | content
        #[arg(long, default_value = "content")]
        kind: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// List skills in a project
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Print a skill's content verbatim
    Show {
        name: String,
        #[arg(long)]
        project: Option<String>,
    },
}

/// Same resolution rule as launch: flag → KORE_PROJECT → "default".
fn project_or_default(p: Option<String>) -> String {
    p.or_else(|| std::env::var("KORE_PROJECT").ok())
        .unwrap_or_else(|| "default".into())
}

async fn post_json<B: serde::Serialize>(path: &str, body: &B) -> anyhow::Result<()> {
    let resp = config::http_client()
        .post(format!("{}{path}", config::server_url()))
        .bearer_auth(config::load_token()?)
        .json(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("request failed ({}): {}", resp.status(), resp.text().await?);
    }
    Ok(())
}

pub async fn role(cmd: RoleCmd) -> anyhow::Result<()> {
    match cmd {
        RoleCmd::Create {
            title,
            content,
            project,
        } => {
            let project = project_or_default(project);
            let body = CreateRoleRequest {
                title: title.clone(),
                content,
            };
            post_json(&format!("/v1/projects/{project}/roles"), &body).await?;
            println!("role '{title}' created");
        }
        RoleCmd::List { project } => {
            let project = project_or_default(project);
            let roles: Vec<RoleSummary> =
                crate::get_json(&format!("/v1/projects/{project}/roles")).await?;
            for r in roles {
                println!("{}", r.title);
            }
        }
    }
    Ok(())
}

pub async fn skill(cmd: SkillCmd) -> anyhow::Result<()> {
    match cmd {
        SkillCmd::Create {
            name,
            content,
            description,
            kind,
            project,
        } => {
            let project = project_or_default(project);
            let body = CreateSkillRequest {
                name: name.clone(),
                description,
                content,
                kind,
            };
            post_json(&format!("/v1/projects/{project}/skills"), &body).await?;
            println!("skill '{name}' created");
        }
        SkillCmd::List { project } => {
            let project = project_or_default(project);
            let skills: Vec<SkillSummary> =
                crate::get_json(&format!("/v1/projects/{project}/skills")).await?;
            for s in skills {
                if s.description.is_empty() {
                    println!("{}", s.name);
                } else {
                    println!("{}  {}", s.name, s.description);
                }
            }
        }
        SkillCmd::Show { name, project } => {
            let project = project_or_default(project);
            let skills: Vec<SkillSummary> =
                crate::get_json(&format!("/v1/projects/{project}/skills")).await?;
            match skills.into_iter().find(|s| s.name == name) {
                Some(s) => print!("{}", s.content),
                None => {
                    eprintln!("unknown skill '{name}'");
                    std::process::exit(1);
                }
            }
        }
    }
    Ok(())
}
