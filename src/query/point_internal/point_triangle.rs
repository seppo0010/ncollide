use na::{self, Real};

use math::{Isometry, Point, Vector};
use query::{PointProjection, PointQuery, PointQueryWithLocation};
use shape::{FeatureId, Triangle, TrianglePointLocation};
use utils::IsometryOps;

#[inline]
fn compute_result<N: Real>(pt: &Point<N>, proj: Point<N>) -> PointProjection<N> {
    #[cfg(feature = "dim2")]
    {
        PointProjection::new(*pt == proj, proj)
    }

    #[cfg(feature = "dim3")]
    {
        // FIXME: is this acceptable to assume the point is inside of the
        // triangle if it is close enough?
        PointProjection::new(relative_eq!(proj, *pt), proj)
    }
}

impl<N: Real> PointQuery<N> for Triangle<N> {
    #[inline]
    fn project_point(&self, m: &Isometry<N>, pt: &Point<N>, solid: bool) -> PointProjection<N> {
        let (projection, _) = self.project_point_with_location(m, pt, solid);
        projection
    }

    #[inline]
    fn project_point_with_feature(
        &self,
        m: &Isometry<N>,
        pt: &Point<N>,
    ) -> (PointProjection<N>, FeatureId) {
        let (proj, loc) = if na::dimension::<Vector<N>>() == 2 {
            self.project_point_with_location(m, pt, false)
        } else {
            self.project_point_with_location(m, pt, true)
        };

        let feature = match loc {
            TrianglePointLocation::OnVertex(i) => FeatureId::Vertex(i),
            #[cfg(feature = "dim3")]
            TrianglePointLocation::OnEdge(i, _) => FeatureId::Edge(i),
            #[cfg(feature = "dim2")]
            TrianglePointLocation::OnEdge(i, _) => FeatureId::Face(i),
            TrianglePointLocation::OnFace(_) => FeatureId::Face(0),
            TrianglePointLocation::OnSolid => FeatureId::Face(0),
        };

        (proj, feature)
    }

    // NOTE: the default implementation of `.distance_to_point(...)` will return the error that was
    // eaten by the `::approx_eq(...)` on `project_point(...)`.
}

impl<N: Real> PointQueryWithLocation<N> for Triangle<N> {
    type Location = TrianglePointLocation<N>;

    #[inline]
    fn project_point_with_location(
        &self,
        m: &Isometry<N>,
        pt: &Point<N>,
        solid: bool,
    ) -> (PointProjection<N>, Self::Location) {
        let a = *self.a();
        let b = *self.b();
        let c = *self.c();
        let p = m.inverse_transform_point(pt);

        let _1 = na::one::<N>();

        let ab = b - a;
        let ac = c - a;
        let ap = p - a;

        let ab_ap = na::dot(&ab, &ap);
        let ac_ap = na::dot(&ac, &ap);

        if ab_ap <= na::zero() && ac_ap <= na::zero() {
            // Voronoï region of `a`.
            return (
                compute_result(pt, m * a),
                TrianglePointLocation::OnVertex(0),
            );
        }

        let bp = p - b;
        let ab_bp = na::dot(&ab, &bp);
        let ac_bp = na::dot(&ac, &bp);

        if ab_bp >= na::zero() && ac_bp <= ab_bp {
            // Voronoï region of `b`.
            return (
                compute_result(pt, m * b),
                TrianglePointLocation::OnVertex(1),
            );
        }

        let cp = p - c;
        let ab_cp = na::dot(&ab, &cp);
        let ac_cp = na::dot(&ac, &cp);

        if ac_cp >= na::zero() && ab_cp <= ac_cp {
            // Voronoï region of `c`.
            return (
                compute_result(pt, m * c),
                TrianglePointLocation::OnVertex(2),
            );
        }

        enum ProjectionInfo<N> {
            OnAB,
            OnAC,
            OnBC,
            OnFace(N, N, N),
        }

        // Checks on which edge voronoï region the point is.
        // For 2D and 3D, it uses explicit cross/perp products that are
        // more numerically stable.
        fn stable_check_edges_voronoi<N: Real>(
            ab: &Vector<N>,
            ac: &Vector<N>,
            bc: &Vector<N>,
            ap: &Vector<N>,
            bp: &Vector<N>,
            cp: &Vector<N>,
            ab_ap: N,
            ab_bp: N,
            ac_ap: N,
            ac_cp: N,
            ac_bp: N,
            ab_cp: N,
        ) -> ProjectionInfo<N> {
            #[cfg(feature = "dim2")]
            {
                let n = ab.perp(&ac);
                let vc = n * ab.perp(&ap);
                if vc < na::zero() && ab_ap >= na::zero() && ab_bp <= na::zero() {
                    return ProjectionInfo::OnAB;
                }

                let vb = -n * ac.perp(&cp);
                if vb < na::zero() && ac_ap >= na::zero() && ac_cp <= na::zero() {
                    return ProjectionInfo::OnAC;
                }

                let va = n * bc.perp(&bp);
                if va < na::zero() && ac_bp - ab_bp >= na::zero() && ab_cp - ac_cp >= na::zero() {
                    return ProjectionInfo::OnBC;
                }

                return ProjectionInfo::OnFace(va, vb, vc);
            }
            #[cfg(feature = "dim3")]
            {
                let n = ab.cross(&ac);
                let vc = na::dot(&n, &ab.cross(&ap));
                if vc < na::zero() && ab_ap >= na::zero() && ab_bp <= na::zero() {
                    return ProjectionInfo::OnAB;
                }

                let vb = -na::dot(&n, &ac.cross(&cp));
                if vb < na::zero() && ac_ap >= na::zero() && ac_cp <= na::zero() {
                    return ProjectionInfo::OnAC;
                }

                let va = na::dot(&n, &bc.cross(&bp));
                if va < na::zero() && ac_bp - ab_bp >= na::zero() && ab_cp - ac_cp >= na::zero() {
                    return ProjectionInfo::OnBC;
                }

                return ProjectionInfo::OnFace(va, vb, vc);
            }
        }

        let bc = c - b;
        match stable_check_edges_voronoi(
            &ab,
            &ac,
            &bc,
            &ap,
            &bp,
            &cp,
            ab_ap,
            ab_bp,
            ac_ap,
            ac_cp,
            ac_bp,
            ab_cp,
        ) {
            ProjectionInfo::OnAB => {
                // Voronoï region of `ab`.
                let v = ab_ap / na::norm_squared(&ab);
                let bcoords = [_1 - v, v];

                let res = a + ab * v;
                return (
                    compute_result(pt, m * res),
                    TrianglePointLocation::OnEdge(0, bcoords),
                );
            }
            ProjectionInfo::OnAC => {
                // Voronoï region of `ac`.
                let w = ac_ap / na::norm_squared(&ac);
                let bcoords = [_1 - w, w];

                let res = a + ac * w;
                return (
                    compute_result(pt, m * res),
                    TrianglePointLocation::OnEdge(2, bcoords),
                );
            }
            ProjectionInfo::OnBC => {
                // Voronoï region of `bc`.
                let w = na::dot(&bc, &bp) / na::norm_squared(&bc);
                let bcoords = [_1 - w, w];

                let res = b + bc * w;
                return (
                    compute_result(pt, m * res),
                    TrianglePointLocation::OnEdge(1, bcoords),
                );
            }
            ProjectionInfo::OnFace(va, vb, vc) => {
                // Voronoï region of the face.
                if na::dimension::<Vector<N>>() != 2 {
                    let denom = _1 / (va + vb + vc);
                    let v = vb * denom;
                    let w = vc * denom;
                    let bcoords = [_1 - v - w, v, w];

                    let res = a + ab * v + ac * w;

                    return (
                        compute_result(pt, m * res),
                        TrianglePointLocation::OnFace(bcoords),
                    );
                }
            }
        }

        // Special treatement if we work in 2d because in this case we really are inside of the
        // object.
        if solid {
            (
                PointProjection::new(true, *pt),
                TrianglePointLocation::OnSolid,
            )
        } else {
            // We have to project on the closest edge.

            // FIXME: this might be optimizable.
            // FIXME: be careful with numerical errors.
            let v = ab_ap / (ab_ap - ab_bp); // proj on ab = a + ab * v
            let w = ac_ap / (ac_ap - ac_cp); // proj on ac = a + ac * w
            let u = (ac_bp - ab_bp) / (ac_bp - ab_bp + ab_cp - ac_cp); // proj on bc = b + bc * u

            let bc = c - b;
            let d_ab = na::norm_squared(&ap) - (na::norm_squared(&ab) * v * v);
            let d_ac = na::norm_squared(&ap) - (na::norm_squared(&ac) * u * u);
            let d_bc = na::norm_squared(&bp) - (na::norm_squared(&bc) * w * w);

            let mut proj;
            let loc;

            if d_ab < d_ac {
                if d_ab < d_bc {
                    // ab
                    let bcoords = [_1 - v, v];
                    proj = a + ab * v;
                    proj = m * proj;
                    loc = TrianglePointLocation::OnEdge(0, bcoords);
                } else {
                    // bc
                    let bcoords = [_1 - u, u];
                    proj = b + bc * u;
                    proj = m * proj;
                    loc = TrianglePointLocation::OnEdge(1, bcoords);
                }
            } else {
                if d_ac < d_bc {
                    // ac
                    let bcoords = [_1 - w, w];
                    proj = a + ac * w;
                    proj = m * proj;
                    loc = TrianglePointLocation::OnEdge(2, bcoords);
                } else {
                    // bc
                    let bcoords = [_1 - u, u];
                    proj = b + bc * u;
                    proj = m * proj;
                    loc = TrianglePointLocation::OnEdge(1, bcoords);
                }
            }

            (PointProjection::new(true, proj), loc)
        }
    }
}
