use anyhow::Result;
use nixpacks::{
    create_docker_image,
    nixpacks::{builder::docker::DockerBuilderOptions, plan::generator::GeneratePlanOptions},
};

#[tokio::main]
async fn main() -> Result<()> {
    let plan_options = GeneratePlanOptions::default();
    let build_options = DockerBuilderOptions {
        name: Some("go-example".to_string()),
        quiet: false,
        ..Default::default()
    };
    let envs = vec![];
    create_docker_image("./src/go-example", envs, &plan_options, &build_options).await
}
