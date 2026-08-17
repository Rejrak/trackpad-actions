use std::collections::HashSet;

pub type ContactId = i32;

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub id: ContactId,
    /// Normalized horizontal coordinate in the range 0.0..=1.0.
    pub x: f64,
    /// Normalized vertical coordinate in the range 0.0..=1.0.
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TouchFrame {
    pub timestamp_us: u64,
    pub contacts: Vec<Contact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GesturePhase {
    Started,
    Updated,
    Ended,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GestureEvent {
    pub gesture_id: String,
    pub phase: GesturePhase,
    /// Vertical displacement from the gesture start. Positive means moving up.
    pub delta: f64,
}

pub trait GestureRecognizer: Send {
    fn id(&self) -> &str;
    fn process(&mut self, frame: &TouchFrame) -> Vec<GestureEvent>;
}

#[derive(Debug, Clone, Copy)]
struct ActiveGesture {
    contact_id: ContactId,
    start_y: f64,
}

/// Recognizes one-finger vertical swipes that *start* on a physical trackpad edge.
///
/// `activation_width` is the edge zone, e.g. 0.06 means the outer 6%.
/// `cancel_margin` adds hysteresis: once active, the finger may drift further inward
/// before the gesture is cancelled.
pub struct EdgeSwipeRecognizer {
    id: String,
    edge: Edge,
    activation_width: f64,
    cancel_margin: f64,
    active: Option<ActiveGesture>,
    previous_contacts: HashSet<ContactId>,
}

impl EdgeSwipeRecognizer {
    pub fn new(
        id: impl Into<String>,
        edge: Edge,
        activation_width: f64,
        cancel_margin: f64,
    ) -> Self {
        Self {
            id: id.into(),
            edge,
            activation_width: activation_width.clamp(0.001, 0.49),
            cancel_margin: cancel_margin.clamp(0.0, 0.49),
            active: None,
            previous_contacts: HashSet::new(),
        }
    }

    fn is_in_activation_zone(&self, x: f64) -> bool {
        match self.edge {
            Edge::Left => x <= self.activation_width,
            Edge::Right => x >= 1.0 - self.activation_width,
        }
    }

    fn is_in_tracking_zone(&self, x: f64) -> bool {
        let tracking_width = (self.activation_width + self.cancel_margin).min(0.49);
        match self.edge {
            Edge::Left => x <= tracking_width,
            Edge::Right => x >= 1.0 - tracking_width,
        }
    }
}

impl GestureRecognizer for EdgeSwipeRecognizer {
    fn id(&self) -> &str {
        &self.id
    }

    fn process(&mut self, frame: &TouchFrame) -> Vec<GestureEvent> {
        let current_contacts: HashSet<_> = frame.contacts.iter().map(|c| c.id).collect();
        let mut events = Vec::new();

        if let Some(active) = self.active {
            if let Some(contact) = frame.contacts.iter().find(|c| c.id == active.contact_id) {
                if frame.contacts.len() != 1 {
                    events.push(GestureEvent {
                        gesture_id: self.id.clone(),
                        phase: GesturePhase::Cancelled,
                        delta: active.start_y - contact.y,
                    });
                    self.active = None;
                } else if self.is_in_tracking_zone(contact.x) {
                    events.push(GestureEvent {
                        gesture_id: self.id.clone(),
                        phase: GesturePhase::Updated,
                        delta: active.start_y - contact.y,
                    });
                } else {
                    events.push(GestureEvent {
                        gesture_id: self.id.clone(),
                        phase: GesturePhase::Cancelled,
                        delta: active.start_y - contact.y,
                    });
                    self.active = None;
                }
            } else {
                events.push(GestureEvent {
                    gesture_id: self.id.clone(),
                    phase: GesturePhase::Ended,
                    delta: 0.0,
                });
                self.active = None;
            }
        }

        // Only begin on a new contact, not when an existing finger slides into the edge.
        if self.active.is_none() && frame.contacts.len() == 1 {
            let contact = &frame.contacts[0];
            let is_new_contact = !self.previous_contacts.contains(&contact.id);

            if is_new_contact && self.is_in_activation_zone(contact.x) {
                self.active = Some(ActiveGesture {
                    contact_id: contact.id,
                    start_y: contact.y,
                });
                events.push(GestureEvent {
                    gesture_id: self.id.clone(),
                    phase: GesturePhase::Started,
                    delta: 0.0,
                });
            }
        }

        self.previous_contacts = current_contacts;
        events
    }
}

#[derive(Default)]
pub struct GestureEngine {
    recognizers: Vec<Box<dyn GestureRecognizer>>,
}

impl GestureEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<R>(&mut self, recognizer: R)
    where
        R: GestureRecognizer + 'static,
    {
        self.recognizers.push(Box::new(recognizer));
    }

    pub fn process(&mut self, frame: &TouchFrame) -> Vec<GestureEvent> {
        self.recognizers
            .iter_mut()
            .flat_map(|recognizer| recognizer.process(frame))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: Option<i32>, x: f64, y: f64) -> TouchFrame {
        TouchFrame {
            timestamp_us: 0,
            contacts: id
                .map(|id| vec![Contact { id, x, y }])
                .unwrap_or_default(),
        }
    }

    #[test]
    fn right_edge_swipe_starts_updates_and_ends() {
        let mut recognizer = EdgeSwipeRecognizer::new("right-edge", Edge::Right, 0.06, 0.04);

        let start = recognizer.process(&frame(Some(7), 0.97, 0.80));
        assert_eq!(start[0].phase, GesturePhase::Started);

        let update = recognizer.process(&frame(Some(7), 0.96, 0.60));
        assert_eq!(update[0].phase, GesturePhase::Updated);
        assert!((update[0].delta - 0.20).abs() < 1e-9);

        let end = recognizer.process(&frame(None, 0.0, 0.0));
        assert_eq!(end[0].phase, GesturePhase::Ended);
    }

    #[test]
    fn existing_contact_entering_edge_does_not_start() {
        let mut recognizer = EdgeSwipeRecognizer::new("right-edge", Edge::Right, 0.06, 0.04);

        assert!(recognizer.process(&frame(Some(9), 0.50, 0.60)).is_empty());
        assert!(recognizer.process(&frame(Some(9), 0.98, 0.50)).is_empty());
    }

    #[test]
    fn leaving_tracking_zone_cancels() {
        let mut recognizer = EdgeSwipeRecognizer::new("left-edge", Edge::Left, 0.06, 0.04);

        recognizer.process(&frame(Some(3), 0.03, 0.80));
        let events = recognizer.process(&frame(Some(3), 0.20, 0.70));

        assert_eq!(events[0].phase, GesturePhase::Cancelled);
    }
}
