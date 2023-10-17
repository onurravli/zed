use crate::{
    black, point, px, Bounds, FontId, Hsla, LineLayout, Pixels, Point, ShapedBoundary, ShapedRun,
    UnderlineStyle, WindowContext,
};
use anyhow::Result;
use smallvec::SmallVec;
use std::sync::Arc;

#[derive(Default, Debug, Clone)]
pub struct Line {
    layout: Arc<LineLayout>,
    decoration_runs: SmallVec<[DecorationRun; 32]>,
}

#[derive(Debug, Clone)]
pub struct DecorationRun {
    pub len: u32,
    pub color: Hsla,
    pub underline: Option<UnderlineStyle>,
}

impl Line {
    pub fn new(layout: Arc<LineLayout>, decoration_runs: SmallVec<[DecorationRun; 32]>) -> Self {
        Self {
            layout,
            decoration_runs,
        }
    }

    pub fn runs(&self) -> &[ShapedRun] {
        &self.layout.runs
    }

    pub fn width(&self) -> Pixels {
        self.layout.width
    }

    pub fn font_size(&self) -> Pixels {
        self.layout.font_size
    }

    pub fn x_for_index(&self, index: usize) -> Pixels {
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index >= index {
                    return glyph.position.x;
                }
            }
        }
        self.layout.width
    }

    pub fn font_for_index(&self, index: usize) -> Option<FontId> {
        for run in &self.layout.runs {
            for glyph in &run.glyphs {
                if glyph.index >= index {
                    return Some(run.font_id);
                }
            }
        }

        None
    }

    pub fn len(&self) -> usize {
        self.layout.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layout.text.is_empty()
    }

    pub fn index_for_x(&self, x: Pixels) -> Option<usize> {
        if x >= self.layout.width {
            None
        } else {
            for run in self.layout.runs.iter().rev() {
                for glyph in run.glyphs.iter().rev() {
                    if glyph.position.x <= x {
                        return Some(glyph.index);
                    }
                }
            }
            Some(0)
        }
    }

    pub fn paint(
        &self,
        bounds: Bounds<Pixels>,
        visible_bounds: Bounds<Pixels>, // todo!("use clipping")
        line_height: Pixels,
        cx: &mut WindowContext,
    ) -> Result<()> {
        let origin = bounds.origin;
        let padding_top = (line_height - self.layout.ascent - self.layout.descent) / 2.;
        let baseline_offset = point(px(0.), padding_top + self.layout.ascent);

        let mut style_runs = self.decoration_runs.iter();
        let mut run_end = 0;
        let mut color = black();
        let mut current_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
        let text_system = cx.text_system().clone();

        for run in &self.layout.runs {
            let max_glyph_width = text_system
                .bounding_box(run.font_id, self.layout.font_size)?
                .size
                .width;

            for glyph in &run.glyphs {
                let glyph_origin = origin + baseline_offset + glyph.position;
                if glyph_origin.x > visible_bounds.upper_right().x {
                    break;
                }

                let mut finished_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
                if glyph.index >= run_end {
                    if let Some(style_run) = style_runs.next() {
                        if let Some((_, underline_style)) = &mut current_underline {
                            if style_run.underline.as_ref() != Some(underline_style) {
                                finished_underline = current_underline.take();
                            }
                        }
                        if let Some(run_underline) = style_run.underline.as_ref() {
                            current_underline.get_or_insert((
                                point(
                                    glyph_origin.x,
                                    origin.y + baseline_offset.y + (self.layout.descent * 0.618),
                                ),
                                UnderlineStyle {
                                    color: Some(run_underline.color.unwrap_or(style_run.color)),
                                    thickness: run_underline.thickness,
                                    wavy: run_underline.wavy,
                                },
                            ));
                        }

                        run_end += style_run.len as usize;
                        color = style_run.color;
                    } else {
                        run_end = self.layout.text.len();
                        finished_underline = current_underline.take();
                    }
                }

                if glyph_origin.x + max_glyph_width < visible_bounds.origin.x {
                    continue;
                }

                if let Some((underline_origin, underline_style)) = finished_underline {
                    cx.paint_underline(
                        underline_origin,
                        glyph_origin.x - underline_origin.x,
                        &underline_style,
                    )?;
                }

                if glyph.is_emoji {
                    cx.paint_emoji(glyph_origin, run.font_id, glyph.id, self.layout.font_size)?;
                } else {
                    cx.paint_glyph(
                        glyph_origin,
                        run.font_id,
                        glyph.id,
                        self.layout.font_size,
                        color,
                    )?;
                }
            }
        }

        if let Some((underline_start, underline_style)) = current_underline.take() {
            let line_end_x = origin.x + self.layout.width;
            cx.paint_underline(
                underline_start,
                line_end_x - underline_start.x,
                &underline_style,
            )?;
        }

        Ok(())
    }

    pub fn paint_wrapped(
        &self,
        origin: Point<Pixels>,
        _visible_bounds: Bounds<Pixels>, // todo!("use clipping")
        line_height: Pixels,
        boundaries: &[ShapedBoundary],
        cx: &mut WindowContext,
    ) -> Result<()> {
        let padding_top = (line_height - self.layout.ascent - self.layout.descent) / 2.;
        let baseline_offset = point(px(0.), padding_top + self.layout.ascent);

        let mut boundaries = boundaries.into_iter().peekable();
        let mut color_runs = self.decoration_runs.iter();
        let mut style_run_end = 0;
        let mut _color = black(); // todo!
        let mut current_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;

        let mut glyph_origin = origin;
        let mut prev_position = px(0.);
        for (run_ix, run) in self.layout.runs.iter().enumerate() {
            for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                glyph_origin.x += glyph.position.x - prev_position;

                if boundaries
                    .peek()
                    .map_or(false, |b| b.run_ix == run_ix && b.glyph_ix == glyph_ix)
                {
                    boundaries.next();
                    if let Some((underline_origin, underline_style)) = current_underline.take() {
                        cx.paint_underline(
                            underline_origin,
                            glyph_origin.x - underline_origin.x,
                            &underline_style,
                        )?;
                    }

                    glyph_origin = point(origin.x, glyph_origin.y + line_height);
                }
                prev_position = glyph.position.x;

                let mut finished_underline = None;
                if glyph.index >= style_run_end {
                    if let Some(style_run) = color_runs.next() {
                        style_run_end += style_run.len as usize;
                        _color = style_run.color;
                        if let Some((_, underline_style)) = &mut current_underline {
                            if style_run.underline.as_ref() != Some(underline_style) {
                                finished_underline = current_underline.take();
                            }
                        }
                        if let Some(underline_style) = style_run.underline.as_ref() {
                            current_underline.get_or_insert((
                                glyph_origin
                                    + point(
                                        px(0.),
                                        baseline_offset.y + (self.layout.descent * 0.618),
                                    ),
                                UnderlineStyle {
                                    color: Some(underline_style.color.unwrap_or(style_run.color)),
                                    thickness: underline_style.thickness,
                                    wavy: underline_style.wavy,
                                },
                            ));
                        }
                    } else {
                        style_run_end = self.layout.text.len();
                        _color = black();
                        finished_underline = current_underline.take();
                    }
                }

                if let Some((underline_origin, underline_style)) = finished_underline {
                    cx.paint_underline(
                        underline_origin,
                        glyph_origin.x - underline_origin.x,
                        &underline_style,
                    )?;
                }

                let text_system = cx.text_system();
                let _glyph_bounds = Bounds {
                    origin: glyph_origin,
                    size: text_system
                        .bounding_box(run.font_id, self.layout.font_size)?
                        .size,
                };
                // if glyph_bounds.intersects(visible_bounds) {
                //     if glyph.is_emoji {
                //         cx.scene().push_image_glyph(scene::ImageGlyph {
                //             font_id: run.font_id,
                //             font_size: self.layout.font_size,
                //             id: glyph.id,
                //             origin: glyph_bounds.origin() + baseline_offset,
                //         });
                //     } else {
                //         cx.scene().push_glyph(scene::Glyph {
                //             font_id: run.font_id,
                //             font_size: self.layout.font_size,
                //             id: glyph.id,
                //             origin: glyph_bounds.origin() + baseline_offset,
                //             color,
                //         });
                //     }
                // }
            }
        }

        if let Some((underline_origin, underline_style)) = current_underline.take() {
            let line_end_x = glyph_origin.x + self.layout.width - prev_position;
            cx.paint_underline(
                underline_origin,
                line_end_x - underline_origin.x,
                &underline_style,
            )?;
        }

        Ok(())
    }
}
