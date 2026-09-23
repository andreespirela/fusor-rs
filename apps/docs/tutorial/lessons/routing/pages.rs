use fusor::FromInputs;

#[derive(FromInputs)]
pub struct Home;
#[derive(FromInputs)]
pub struct NotFound;
#[derive(FromInputs)]
pub struct Article {
    #[input]
    pub id: String,
}

fusor::template!("web/components/pages.html");
