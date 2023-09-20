use std::collections::HashMap;

use anyhow::Result;
use bollard::{container::StartContainerOptions, image::ListImagesOptions, Docker};
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
    create_docker_image("./src/go-example", envs, &plan_options, &build_options).await?;

    let docker = Docker::connect_with_local_defaults()?;

    let images = &docker
        .list_images(Some(ListImagesOptions::<String> {
            all: false,
            filters: HashMap::from([(
                "reference".to_string(),
                vec!["go-example:latest".to_string()],
            )]),
            ..Default::default()
        }))
        .await
        .unwrap();

    for image in images {
        println!("-> {:#?}", image);
    }

    let _image = images.first().ok_or(anyhow::anyhow!("No image found"))?;

    docker
        .start_container("go-example", None::<StartContainerOptions<String>>)
        .await?;

    Ok(())
}
