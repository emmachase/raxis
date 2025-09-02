use windows_numerics::Vector2;

#[derive(Debug, Clone, Copy)]
pub enum PathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    ClosePath,
    /// Arc command: end_point, radius_x, radius_y, rotation_angle, large_arc_flag, sweep_flag
    Arc {
        radius_x: f32,
        radius_y: f32,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        end_x: f32,
        end_y: f32,
    },
    /// Cubic Bezier: control1, control2, end_point
    CubicBezier {
        cp1_x: f32,
        cp1_y: f32,
        cp2_x: f32,
        cp2_y: f32,
        end_x: f32,
        end_y: f32,
    },
    /// Quadratic Bezier: control_point, end_point
    QuadraticBezier {
        cp_x: f32,
        cp_y: f32,
        end_x: f32,
        end_y: f32,
    },
}

#[derive(Debug)]
pub struct SvgPathCommands {
    pub commands: &'static [PathCommand],
}

impl SvgPathCommands {
    /// Creates a Direct2D path geometry from this SVG path
    pub fn create_geometry(
        &self,
        factory: &windows::Win32::Graphics::Direct2D::ID2D1Factory,
    ) -> windows::core::Result<windows::Win32::Graphics::Direct2D::ID2D1PathGeometry> {
        use windows::Win32::Graphics::Direct2D::{Common::*, *};

        unsafe {
            let path_geometry: ID2D1PathGeometry = factory.CreatePathGeometry()?;

            let sink: ID2D1GeometrySink = path_geometry.Open()?;

            sink.SetFillMode(D2D1_FILL_MODE_WINDING);

            let mut points = Vec::new();
            let mut has_begin_figure = false;

            for command in self.commands {
                match command {
                    PathCommand::MoveTo { x, y } => {
                        if has_begin_figure {
                            // End previous figure and start new one
                            if !points.is_empty() {
                                let point_array: Vec<Vector2> = points
                                    .iter()
                                    .map(|(px, py)| Vector2 { X: *px, Y: *py })
                                    .collect();
                                sink.AddLines(&point_array);
                                points.clear();
                            }
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }

                        sink.BeginFigure(Vector2 { X: *x, Y: *y }, D2D1_FIGURE_BEGIN_FILLED);
                        has_begin_figure = true;
                    }
                    PathCommand::LineTo { x, y } => {
                        points.push((*x, *y));
                    }
                    PathCommand::Arc {
                        end_x,
                        end_y,
                        radius_x,
                        radius_y,
                        rotation,
                        large_arc,
                        sweep,
                    } => {
                        // Add accumulated points first
                        if !points.is_empty() {
                            let point_array: Vec<Vector2> = points
                                .iter()
                                .map(|(px, py)| Vector2 { X: *px, Y: *py })
                                .collect();
                            sink.AddLines(&point_array);
                            points.clear();
                        }

                        let arc_segment = D2D1_ARC_SEGMENT {
                            point: Vector2 {
                                X: *end_x,
                                Y: *end_y,
                            },
                            size: D2D_SIZE_F {
                                width: *radius_x,
                                height: *radius_y,
                            },
                            rotationAngle: *rotation,
                            sweepDirection: if *sweep {
                                D2D1_SWEEP_DIRECTION_CLOCKWISE
                            } else {
                                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                            },
                            arcSize: if *large_arc {
                                D2D1_ARC_SIZE_LARGE
                            } else {
                                D2D1_ARC_SIZE_SMALL
                            },
                        };
                        sink.AddArc(&arc_segment);
                    }
                    PathCommand::CubicBezier {
                        cp1_x,
                        cp1_y,
                        cp2_x,
                        cp2_y,
                        end_x,
                        end_y,
                    } => {
                        // Add accumulated points first
                        if !points.is_empty() {
                            let point_array: Vec<Vector2> = points
                                .iter()
                                .map(|(px, py)| Vector2 { X: *px, Y: *py })
                                .collect();
                            sink.AddLines(&point_array);
                            points.clear();
                        }

                        let bezier_segment = D2D1_BEZIER_SEGMENT {
                            point1: Vector2 {
                                X: *cp1_x,
                                Y: *cp1_y,
                            },
                            point2: Vector2 {
                                X: *cp2_x,
                                Y: *cp2_y,
                            },
                            point3: Vector2 {
                                X: *end_x,
                                Y: *end_y,
                            },
                        };
                        sink.AddBezier(&bezier_segment);
                    }
                    PathCommand::QuadraticBezier {
                        cp_x,
                        cp_y,
                        end_x,
                        end_y,
                    } => {
                        // Add accumulated points first
                        if !points.is_empty() {
                            let point_array: Vec<Vector2> = points
                                .iter()
                                .map(|(px, py)| Vector2 { X: *px, Y: *py })
                                .collect();
                            sink.AddLines(&point_array);
                            points.clear();
                        }

                        let quad_bezier_segment = D2D1_QUADRATIC_BEZIER_SEGMENT {
                            point1: Vector2 { X: *cp_x, Y: *cp_y },
                            point2: Vector2 {
                                X: *end_x,
                                Y: *end_y,
                            },
                        };
                        sink.AddQuadraticBezier(&quad_bezier_segment);
                    }
                    PathCommand::ClosePath => {
                        // Add accumulated points before closing
                        if !points.is_empty() {
                            let point_array: Vec<Vector2> = points
                                .iter()
                                .map(|(px, py)| Vector2 { X: *px, Y: *py })
                                .collect();
                            sink.AddLines(&point_array);
                            points.clear();
                        }

                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        has_begin_figure = false;
                    }
                }
            }

            // Add remaining points if any
            if !points.is_empty() {
                let point_array: Vec<Vector2> = points
                    .iter()
                    .map(|(px, py)| Vector2 { X: *px, Y: *py })
                    .collect();
                sink.AddLines(&point_array);
            }

            // End figure if still open
            if has_begin_figure {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }

            sink.Close()?;

            Ok(path_geometry)
        }
    }
}
