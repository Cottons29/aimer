use attribute::dimension::Dimension;
use widget::base::Colors;
use widget::Widget;


#[derive(Clone)]
pub struct ScrollTrack {
    pub width: Dimension,
    pub color: Colors,
    pub hover_color: Colors,
}

impl Default for ScrollTrack {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            color: Colors::Transparent,
            hover_color: Colors::Transparent,
        }
    }
}

#[derive(Clone)]
pub struct ScrollThumb {
    pub width: Dimension,
    pub radius: Dimension,
    pub color: Colors,
    pub hover_color: Colors,
    pub active_color: Colors,
}

impl Default for ScrollThumb {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            radius: Dimension::Px(4.0),
            color: Colors::RGBA(150, 150, 150, 150),
            hover_color: Colors::RGBA(100, 100, 100, 200),
            active_color: Colors::RGBA(80, 80, 80, 255),
        }
    }
}

#[derive(Clone)]
pub struct ScrollButton {
    pub width: Dimension,
    pub height: Dimension,

    pub color: Colors,
    pub hover_color: Colors,
    pub active_color: Colors,
}

#[derive(Clone)]
pub struct ScrollBar {
    pub track: ScrollTrack,
    pub thumb: ScrollThumb,
    pub up_button: Option<ScrollButton>,
    pub down_button: Option<ScrollButton>,
}

impl Default for ScrollBar {
    fn default() -> Self {
        Self {
            track: ScrollTrack::default(),
            thumb: ScrollThumb::default(),
            up_button: None,
            down_button: None,
        }
    }
}