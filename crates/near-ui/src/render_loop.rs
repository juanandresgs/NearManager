#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RenderInvalidation {
    pending: bool,
}

impl RenderInvalidation {
    pub(crate) const fn initial() -> Self {
        Self { pending: true }
    }

    pub(crate) const fn request(&mut self) {
        self.pending = true;
    }

    pub(crate) const fn request_if(&mut self, changed: bool) {
        if changed {
            self.request();
        }
    }

    pub(crate) const fn take(&mut self) -> bool {
        let pending = self.pending;
        self.pending = false;
        pending
    }
}

#[cfg(test)]
mod tests {
    use super::RenderInvalidation;

    #[test]
    fn rendering_occurs_initially_and_only_after_invalidation() {
        let mut redraw = RenderInvalidation::initial();
        assert!(redraw.take());
        assert!(!redraw.take());
        redraw.request_if(false);
        assert!(!redraw.take());
        redraw.request();
        assert!(redraw.take());
        assert!(!redraw.take());
    }
}
