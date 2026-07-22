pub struct BackendApi;

const BASE_API: &str = "http://localhost:3200";

impl BackendApi {
    pub fn blogs() -> String {
        format!("{}/api/blogs", BASE_API)
    }

    pub fn blog_with_id(id: &str) -> String {
        format!("{BASE_API}/api/blogs/{}", id)
    }
}
