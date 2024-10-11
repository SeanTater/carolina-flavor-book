use anyhow::Result;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};

#[derive(Clone)]
pub struct StorageClient {
    gcs_client: Client,
}

impl StorageClient {
    pub async fn new() -> Result<Self> {
        let config = ClientConfig::default().with_auth().await?;
        let gcs_client = Client::new(config);
        Ok(Self { gcs_client })
    }

    /// Upload a file to the storage bucket.
    pub async fn upload_file(&self, rel_path: &str, content: Vec<u8>) -> Result<()> {
        let gcs_path = format!("gallagher-kitchen/{}", rel_path);
        let media = Media::new(gcs_path);
        let request = UploadObjectRequest {
            bucket: "kibitz-prod".into(),
            ..Default::default()
        };
        self.gcs_client
            .upload_object(&request, content, &UploadType::Simple(media))
            .await?;
        Ok(())
    }

    /// Upload an image to the storage bucket.
    pub async fn upload_image(&self, image_id: i64, image: Vec<u8>) -> Result<()> {
        self.upload_file(&format!("images/{}.webp", image_id), image)
            .await
    }
}
