use crate::{
    context::Context,
    error::Result,
    pipeline::{
        self,
        cargo::{Mode, compile},
    },
    workspace::Project,
};

pub(crate) fn run(cx: &Context, project: &Project) -> Result {
    if project.config.delivery.is_some() {
        return pipeline::islands::check(cx, project);
    }
    compile(cx, project, Mode::Check).map(drop)
}
