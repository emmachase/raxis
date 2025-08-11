use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_SIZE_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN},
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    ID2D1Factory, ID2D1PathGeometry,
};
use windows_numerics::Vector2;

// A helper to construct circular arc geometry from center/radius and begin/end angles (in degrees, clockwise).
// Angles are measured clockwise from the positive X axis in DIPs to match screen coordinates (y increases downward).
pub struct CircleArc {
    center: Vector2,
    radius: f32,
    begin_deg: f32,
    end_deg: f32,
}

const DRAW_EPSILON: f32 = 1e-6;

impl CircleArc {
    pub fn new(center: Vector2, radius: f32, begin_deg: f32, end_deg: f32) -> Self {
        Self {
            center,
            radius,
            begin_deg,
            end_deg,
        }
    }

    pub fn paint(&self, factory: &ID2D1Factory) -> windows::core::Result<ID2D1PathGeometry> {
        unsafe {
            let path: ID2D1PathGeometry = factory.CreatePathGeometry()?;
            {
                let sink = path.Open()?;

                // Normalize clockwise delta to [0, 360)
                let mut cw_delta = (self.end_deg - self.begin_deg) % 360.0;
                if cw_delta < 0.0 {
                    cw_delta += 360.0;
                }

                let start_rad = self.begin_deg.to_radians();
                let end_rad = self.end_deg.to_radians();

                let start = Vector2 {
                    X: self.center.X + self.radius * start_rad.cos(),
                    Y: self.center.Y + self.radius * start_rad.sin(),
                };
                let end = Vector2 {
                    X: self.center.X + self.radius * end_rad.cos(),
                    Y: self.center.Y + self.radius * end_rad.sin(),
                };

                // Open figure (hollow for stroking)
                sink.BeginFigure(start, D2D1_FIGURE_BEGIN_HOLLOW);

                // Don't emit anything for empty arcs
                if !(f32::abs(self.begin_deg) < DRAW_EPSILON
                    && f32::abs(self.end_deg) < DRAW_EPSILON)
                {
                    if cw_delta.abs() < DRAW_EPSILON {
                        // Full circle: emit two 180-degree arcs back to start
                        let mid_deg = self.begin_deg + 180.0;
                        let mid_rad = mid_deg.to_radians();
                        let mid = Vector2 {
                            X: self.center.X + self.radius * mid_rad.cos(),
                            Y: self.center.Y + self.radius * mid_rad.sin(),
                        };

                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: mid,
                            size: D2D_SIZE_F {
                                width: self.radius,
                                height: self.radius,
                            },
                            rotationAngle: 0.0,
                            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                            arcSize: D2D1_ARC_SIZE_LARGE,
                        });
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: start,
                            size: D2D_SIZE_F {
                                width: self.radius,
                                height: self.radius,
                            },
                            rotationAngle: 0.0,
                            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                            arcSize: D2D1_ARC_SIZE_LARGE,
                        });
                    } else {
                        let arc_size = if cw_delta > 180.0 {
                            D2D1_ARC_SIZE_LARGE
                        } else {
                            D2D1_ARC_SIZE_SMALL
                        };
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: end,
                            size: D2D_SIZE_F {
                                width: self.radius,
                                height: self.radius,
                            },
                            rotationAngle: 0.0,
                            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                            arcSize: arc_size,
                        });
                    }
                }

                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
            }
            Ok(path)
        }
    }
}
