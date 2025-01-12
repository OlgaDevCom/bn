use warp::Filter;
 

pub fn routes() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("static").and(warp::fs::dir("./static"))
}
