/// A point-in-time summary of the active backend's audio output route.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioRouteSnapshot {
    /// Negotiated or server-reported rate when the backend exposes it.
    pub sample_rate_hz: Option<u32>,
    /// Friendly descriptions of connected output destinations.
    pub output_routes: Vec<String>,
}

impl AudioRouteSnapshot {
    pub fn has_output_route(&self) -> bool {
        !self.output_routes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::AudioRouteSnapshot;

    #[test]
    fn route_availability_is_explicit() {
        assert!(!AudioRouteSnapshot::default().has_output_route());
        assert!(
            AudioRouteSnapshot {
                sample_rate_hz: Some(48_000),
                output_routes: vec!["Built-in Audio:playback_FL".to_owned()],
            }
            .has_output_route()
        );
    }
}
