//! CPU renderers of the 3D LUT (port of the scalar paths of `Lut3DOpCPU.cpp`).

#![allow(clippy::needless_range_loop)]

use super::{Lut3DArray, Lut3DOpData};
use crate::ops::lut1d::{clamp_ocio, sanitize_float};
use crate::ops::Pixel;
use crate::types::{Interpolation, TransformDirection};

#[inline]
fn index_blue_fast(r: usize, g: usize, b: usize, dim: usize) -> usize {
    3 * (b + dim * (g + dim * r))
}

/// Linear interpolation of RGB triplets.
#[inline]
fn lerp_rgb(a: &[f32], b: &[f32], z: f32) -> [f32; 3] {
    [
        (b[0] - a[0]) * z + a[0],
        (b[1] - a[1]) * z + a[1],
        (b[2] - a[2]) * z + a[2],
    ]
}

/// Forward renderer data shared by the trilinear and tetrahedral renderers.
#[derive(Debug)]
pub(crate) struct ForwardRenderer {
    lut: Vec<f32>,
    dim: usize,
    step: f32,
    tetrahedral: bool,
}

impl ForwardRenderer {
    fn new(lut: &Lut3DOpData, tetrahedral: bool) -> Self {
        let dim = lut.array().length();
        Self {
            lut: lut
                .array()
                .values()
                .iter()
                .map(|&v| sanitize_float(v))
                .collect(),
            dim,
            step: dim as f32 - 1.0,
            tetrahedral,
        }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let dim_minus_one = self.dim as f32 - 1.0;
        let lut = &self.lut;
        for p in pixels.iter_mut() {
            // NaNs become 0.
            let idx = [
                clamp_ocio(p[0] * self.step, 0.0, dim_minus_one),
                clamp_ocio(p[1] * self.step, 0.0, dim_minus_one),
                clamp_ocio(p[2] * self.step, 0.0, dim_minus_one),
            ];
            let low = [
                idx[0].floor() as usize,
                idx[1].floor() as usize,
                idx[2].floor() as usize,
            ];
            // When idx is exactly an index, high == low but the delta is 0.
            let high = [
                idx[0].ceil() as usize,
                idx[1].ceil() as usize,
                idx[2].ceil() as usize,
            ];
            let fx = idx[0] - low[0] as f32;
            let fy = idx[1] - low[1] as f32;
            let fz = idx[2] - low[2] as f32;

            let d = self.dim;
            let n000 = index_blue_fast(low[0], low[1], low[2], d);
            let n100 = index_blue_fast(high[0], low[1], low[2], d);
            let n010 = index_blue_fast(low[0], high[1], low[2], d);
            let n001 = index_blue_fast(low[0], low[1], high[2], d);
            let n110 = index_blue_fast(high[0], high[1], low[2], d);
            let n101 = index_blue_fast(high[0], low[1], high[2], d);
            let n011 = index_blue_fast(low[0], high[1], high[2], d);
            let n111 = index_blue_fast(high[0], high[1], high[2], d);

            let out = if self.tetrahedral {
                // Weights and corners of the tetrahedron containing the point.
                let (w, c) = if fx > fy {
                    if fy > fz {
                        ([1.0 - fx, fx - fy, fy - fz, fz], [n000, n100, n110, n111])
                    } else if fx > fz {
                        ([1.0 - fx, fx - fz, fz - fy, fy], [n000, n100, n101, n111])
                    } else {
                        ([1.0 - fz, fz - fx, fx - fy, fy], [n000, n001, n101, n111])
                    }
                } else if fz > fy {
                    ([1.0 - fz, fz - fy, fy - fx, fx], [n000, n001, n011, n111])
                } else if fz > fx {
                    ([1.0 - fy, fy - fz, fz - fx, fx], [n000, n010, n011, n111])
                } else {
                    ([1.0 - fy, fy - fx, fx - fz, fz], [n000, n010, n110, n111])
                };
                let ch = |k: usize| {
                    w[0] * lut[c[0] + k]
                        + w[1] * lut[c[1] + k]
                        + w[2] * lut[c[2] + k]
                        + w[3] * lut[c[3] + k]
                };
                [ch(0), ch(1), ch(2)]
            } else {
                // Trilinear: along blue, then green, then red.
                let v = |n: usize| &lut[n..n + 3];
                let b1 = lerp_rgb(v(n000), v(n001), fz);
                let b2 = lerp_rgb(v(n010), v(n011), fz);
                let g1 = lerp_rgb(&b1, &b2, fy);
                let b3 = lerp_rgb(v(n100), v(n101), fz);
                let b4 = lerp_rgb(v(n110), v(n111), fz);
                let g2 = lerp_rgb(&b3, &b4, fy);
                lerp_rgb(&g1, &g2, fx)
            };
            p[0] = out[0];
            p[1] = out[1];
            p[2] = out[2];
            // Alpha is unchanged.
        }
    }
}

// ---------------------------------------------------------------------------
// Inverse.

/// Max input channels.
const MAX_N: usize = 4;
/// Max tree depth.
const MAX_LEVELS: usize = 16;

/// Test a given grid cube of the LUT to see if it contains the inverse (port
/// of `invert_hypercube`, based on an algorithm in "Numerical Linear Algebra
/// and Optimization, vol. 1," by Gill, Murray, and Wright). A customized
/// matrix factorization updating technique is used.
#[allow(clippy::too_many_arguments)]
fn invert_hypercube(
    n: usize,
    x_out: &mut [f32; 3],
    gr: &[f32],
    ind2off: &[usize; 3],
    val: &[f32; 3],
    guess: &[usize; 3],
    ops_list: &[i64],
    entering_list: &[usize],
    new_vert_list: &[usize],
    path_list: &[usize],
    path_order: &[usize],
) -> bool {
    // Singularity tolerance.
    const ZERO_TOL: f64 = 1.0e-9;
    // Feasibility tolerances.
    const NEGZERO_TOL: f64 = -1.0e-9;
    const ONE_TOL: f64 = 1.0 + 1.0e-9;

    let mut row_perm = [0usize; MAX_N];
    let mut col_perm = [0usize; MAX_N];
    let mut sweep_to: Vec<usize> = Vec::with_capacity(32);
    let mut sweep_from: Vec<usize> = Vec::with_capacity(32);
    let mut sweep_f: Vec<f64> = Vec::with_capacity(32);
    let mut base_vert = [0f64; MAX_N];
    let mut y = [0f64; MAX_N];
    let mut u = [[0f64; MAX_N]; MAX_N];
    let mut x = [0f64; MAX_N];
    let mut b = [0f64; MAX_N];
    let mut x2 = [0f64; MAX_N];
    let mut new_vert = [0f64; MAX_N];

    let mut infeas = false;
    let nm1 = n - 1;
    let nm2 = n - 2;

    let mut base_ind = 0usize;
    for i in 0..n {
        base_ind += guess[i] * ind2off[i];
    }

    for i in 0..n {
        row_perm[i] = i;
        col_perm[i] = i;
        base_vert[i] = gr[base_ind + i] as f64;
        b[i] = val[i] as f64 - base_vert[i];
        y[i] = b[i];
        for j in 0..n {
            u[i][j] = if i == j { 1.0 } else { 0.0 };
        }
    }

    for i in 0..ops_list.len() {
        let mut backsub = ops_list[i];
        if backsub < 0 {
            sweep_to.clear();
            sweep_from.clear();
            sweep_f.clear();
            backsub = 0;
            for j in 0..n {
                y[j] = b[j];
                row_perm[j] = j;
                col_perm[j] = j;
                for k in 0..n {
                    u[j][k] = if j == k { 1.0 } else { 0.0 };
                }
            }
        }

        let entering_ind = entering_list[i];
        let tmp_ind = base_ind + n * new_vert_list[i];
        for j in 0..n {
            new_vert[j] = gr[tmp_ind + j] as f64 - base_vert[j];
        }

        for j in 0..sweep_to.len() {
            new_vert[sweep_to[j]] -= sweep_f[j] * new_vert[sweep_from[j]];
        }

        let mut leaving_nz = 0usize;
        for j in 0..n {
            u[j][entering_ind] = new_vert[j];
            if col_perm[j] == entering_ind {
                leaving_nz = j + 1;
            }
        }
        // (col_perm is a permutation, so leaving_nz >= 1.)
        let leaving_nz = leaving_nz.max(1);

        if leaving_nz <= nm2 {
            let tmp = col_perm[leaving_nz - 1];
            for j in (leaving_nz - 1)..nm2 {
                col_perm[j] = col_perm[j + 1];
            }
            col_perm[nm2] = tmp;
        }

        for j in (leaving_nz - 1)..nm1 {
            let jp1 = j + 1;
            let mut piv = j;
            let mut col_piv = j;
            let mut abs_d = u[row_perm[j]][col_perm[j]].abs();
            for k in jp1..n {
                let abs_n = u[row_perm[k]][col_perm[j]].abs();
                if abs_n > abs_d {
                    abs_d = abs_n;
                    piv = k;
                }
            }

            if abs_d < ZERO_TOL {
                // (Always do a rank revealing factorization here, slower but
                // more robust.)
                for h in jp1..n {
                    for k in j..n {
                        let abs_n = u[row_perm[k]][col_perm[h]].abs();
                        if abs_n > abs_d {
                            abs_d = abs_n;
                            piv = k;
                            col_piv = h;
                        }
                    }
                    if abs_d > ZERO_TOL {
                        col_perm.swap(j, col_piv);
                    }
                }
            }
            if piv != j {
                row_perm.swap(j, piv);
            }

            let denom = u[row_perm[j]][col_perm[j]];
            for h in jp1..n {
                let num = u[row_perm[h]][col_perm[j]];
                if num.abs() >= ZERO_TOL {
                    let f = num / denom;
                    u[row_perm[h]][col_perm[j]] = 0.0;
                    for k in jp1..n {
                        u[row_perm[h]][col_perm[k]] -= f * u[row_perm[j]][col_perm[k]];
                    }
                    y[row_perm[h]] -= f * y[row_perm[j]];
                    sweep_to.push(row_perm[h]);
                    sweep_from.push(row_perm[j]);
                    sweep_f.push(f);
                }
            }
        }

        if backsub != 0 {
            let mut running_sumx = 0.0f64;
            for js in (0..n).rev() {
                let denom = u[row_perm[js]][col_perm[js]];
                if denom.abs() < ZERO_TOL {
                    if y[row_perm[js]].abs() > ZERO_TOL {
                        infeas = true;
                        break;
                    } else {
                        x[js] = 0.0;
                        infeas = false;
                    }
                } else {
                    let mut sm = 0.0f64;
                    for k in (js + 1)..n {
                        sm += u[row_perm[js]][col_perm[k]] * x[k];
                    }
                    let x_tmp = (y[row_perm[js]] - sm) / denom;

                    infeas = x_tmp < NEGZERO_TOL;
                    if infeas {
                        break;
                    }
                    running_sumx += x_tmp;
                    infeas = running_sumx > ONE_TOL;
                    if infeas {
                        break;
                    }
                    x[js] = x_tmp;
                }
            }

            if !infeas {
                for j in 0..n {
                    x2[col_perm[j]] = x[j];
                }
                let mut tmp_ind = i * n + n - 1;
                x_out[path_list[tmp_ind]] = x2[path_order[0]] as f32;
                for &po in path_order.iter().take(n).skip(1) {
                    tmp_ind -= 1;
                    x_out[path_list[tmp_ind]] =
                        (x2[po] + x_out[path_list[tmp_ind + 1]] as f64) as f32;
                }
                break;
            }
        }
    }

    if infeas {
        false
    } else {
        for j in 0..n {
            x_out[j] += guess[j] as f32;
        }
        true
    }
}

/// A level of the range tree.
#[derive(Debug, Default)]
struct TreeLevel {
    /// Number of elements on this level.
    elems: usize,
    /// Min LUT value for the sub-tree.
    min_vals: Vec<f32>,
    /// Max LUT value for the sub-tree.
    max_vals: Vec<f32>,
    /// Offsets to the first children.
    child0offsets: Vec<usize>,
    /// Number of children in the sub-tree.
    num_children: Vec<usize>,
}

/// Base grid indices of a LUT cube.
#[derive(Debug, Clone, Copy, Default)]
struct BaseInd {
    inds: [usize; 3],
    hash: usize,
}

/// A modified nd-tree allowing fast range queries in a LUT: since LUT
/// interpolation is a convex operation, the output of a cube must be
/// between the min and max value of its corners for each channel.
#[derive(Debug, Default)]
struct RangeTree {
    chans: usize,
    gsz: [usize; 3],
    depth: usize,
    levels: Vec<TreeLevel>,
    base_inds: Vec<BaseInd>,
    level_scales: Vec<usize>,
}

impl RangeTree {
    /// Populate the tree using the (extrapolated) LUT values.
    fn new(grvec: &[f32], gsz: usize) -> Self {
        let mut t = RangeTree {
            chans: 3,
            gsz: [gsz; 3],
            ..Default::default()
        };

        // Depth of the tree: exponent of frexp(maxGsz - 2).
        let max_gsz = gsz;
        let x = max_gsz.saturating_sub(2);
        t.depth = (usize::BITS - x.leading_zeros()) as usize;
        let depth = t.depth;

        // Size of each level.
        t.levels = (0..depth)
            .map(|i| {
                let elems = (0..t.chans)
                    .map(|j| ((t.gsz[j] - 2) >> (depth - 1 - i)) + 1)
                    .product();
                TreeLevel {
                    elems,
                    ..Default::default()
                }
            })
            .collect();

        // Scale to use for the hash.
        t.level_scales = (0..depth)
            .map(|level| 1usize << ((t.chans + 1) * (depth - 1 - level)))
            .collect();

        t.init_inds();
        for i in 0..t.base_inds.len() {
            t.inds_to_hash(i);
        }
        // Sort indices based on hash (hashes are unique).
        t.base_inds.sort_by_key(|b| b.hash);

        let mut hashes: Vec<usize> = t.base_inds.iter().map(|b| b.hash).collect();

        t.init_ranges(grvec);

        // Start at the bottom of the tree and work up, consolidating levels.
        for level in (0..depth.saturating_sub(1)).rev() {
            t.update_children(&hashes, level);
            let level_size = t.levels[level].elems;
            let new_hashes: Vec<usize> = (0..level_size)
                .map(|i| {
                    hashes
                        .get(t.levels[level].child0offsets[i])
                        .copied()
                        .unwrap_or(0)
                })
                .collect();
            hashes = new_hashes;
            t.update_ranges(level);
        }
        t
    }

    fn init_inds(&mut self) {
        let (i_lim, j_lim, k_lim) = (self.gsz[0] - 1, self.gsz[1] - 1, self.gsz[2] - 1);
        self.base_inds = Vec::with_capacity(i_lim * j_lim * k_lim);
        for i in 0..i_lim {
            for j in 0..j_lim {
                for k in 0..k_lim {
                    self.base_inds.push(BaseInd {
                        inds: [i, j, k],
                        hash: 0,
                    });
                }
            }
        }
    }

    fn inds_to_hash(&mut self, i: usize) {
        const POWS2: [usize; 4] = [1, 2, 4, 8];
        let depthm1 = self.depth - 1;
        let mut hash = 0usize;
        for level in 0..self.depth {
            let mut key_bits = 0usize;
            for (ch, pow) in POWS2.iter().enumerate().take(self.chans) {
                let ind_bit = (self.base_inds[i].inds[ch] >> (depthm1 - level)) & 1;
                key_bits += ind_bit * pow;
            }
            hash += key_bits * self.level_scales[level];
        }
        self.base_inds[i].hash = hash;
    }

    fn init_ranges(&mut self, grvec: &[f32]) {
        let depthm1 = self.depth - 1;
        let chans = self.chans;
        let n = self.levels[depthm1].elems.min(self.base_inds.len());
        // The 3D LUTs are stored with the blue channel varying most rapidly.
        let ind0scale = self.gsz[2] * self.gsz[1];
        let ind1scale = self.gsz[2];
        let g2 = self.gsz[2];
        let g21 = self.gsz[2] * self.gsz[1];
        let corner_offsets = [0, 1, g2, g2 + 1, g21, g21 + 1, g21 + g2, g21 + g2 + 1];

        // Expand the ranges slightly to allow for error in forward evaluation.
        const TOL: f32 = 1e-6;

        let mut min_vals = vec![0f32; n * chans];
        let mut max_vals = vec![0f32; n * chans];
        for i in 0..n {
            let bi = &self.base_inds[i].inds;
            let base_offset = bi[0] * ind0scale + bi[1] * ind1scale + bi[2];
            let mut mn = [0f32; MAX_N];
            let mut mx = [0f32; MAX_N];
            for k in 0..chans {
                mn[k] = grvec[base_offset * chans + k];
                mx[k] = mn[k];
            }
            for off in corner_offsets.iter().skip(1) {
                let index = (base_offset + off) * chans;
                for k in 0..chans {
                    let v = grvec[index + k];
                    // std::min / std::max semantics.
                    mn[k] = if v < mn[k] { v } else { mn[k] };
                    mx[k] = if mx[k] < v { v } else { mx[k] };
                }
            }
            for k in 0..chans {
                min_vals[i * chans + k] = mn[k] - TOL;
                max_vals[i * chans + k] = mx[k] + TOL;
            }
        }
        self.levels[depthm1].min_vals = min_vals;
        self.levels[depthm1].max_vals = max_vals;
    }

    fn update_children(&mut self, hashes: &[usize], level: usize) {
        let level_size = self.levels[level].elems;
        let max_children = 1usize << self.chans;
        let gap = self.level_scales[level + 1] * max_children;

        let mut child0offsets = Vec::with_capacity(level_size);
        child0offsets.push(0usize);
        for i in 1..hashes.len() {
            if hashes[i] - hashes[i - 1] > gap {
                child0offsets.push(i);
            }
        }
        // (The number of parents always matches the level size.)
        child0offsets.resize(level_size, hashes.len());

        let mut num_children = vec![0usize; level_size];
        for i in 0..level_size.saturating_sub(1) {
            num_children[i] = child0offsets[i + 1] - child0offsets[i];
        }
        if level_size > 0 {
            num_children[level_size - 1] = hashes.len() - child0offsets[level_size - 1];
        }
        self.levels[level].child0offsets = child0offsets;
        self.levels[level].num_children = num_children;
    }

    fn update_ranges(&mut self, level: usize) {
        let max_children = 1usize << self.chans;
        let chans = self.chans;
        let level_size = self.levels[level].elems;
        let mut min_vals = vec![0f32; level_size * chans];
        let mut max_vals = vec![0f32; level_size * chans];
        {
            let (upper, lower) = self.levels.split_at(level + 1);
            let cur = &upper[level];
            let next = &lower[0];
            for i in 0..level_size {
                let index = cur.child0offsets[i];
                for k in 0..chans {
                    min_vals[i * chans + k] = next.min_vals[index * chans + k];
                    max_vals[i * chans + k] = next.max_vals[index * chans + k];
                }
                // Combine the min/max of all the children from the next level.
                for j in 2..=max_children {
                    if cur.num_children[i] >= j {
                        let ind = index + j - 1;
                        for k in 0..chans {
                            let child_min = next.min_vals[ind * chans + k];
                            if child_min < min_vals[i * chans + k] {
                                min_vals[i * chans + k] = child_min;
                            }
                            let child_max = next.max_vals[ind * chans + k];
                            if child_max > max_vals[i * chans + k] {
                                max_vals[i * chans + k] = child_max;
                            }
                        }
                    }
                }
            }
        }
        self.levels[level].min_vals = min_vals;
        self.levels[level].max_vals = max_vals;
    }
}

/// Map a value away from the LUT center (used to extrapolate the LUT).
#[inline]
fn extrapolate(rgb: [f32; 3], center: f32, scale: f32) -> [f32; 3] {
    [
        (rgb[0] - center) * scale + center,
        (rgb[1] - center) * scale + center,
        (rgb[2] - center) * scale + center,
    ]
}

/// Extrapolate the 3D LUT by one grid point on each side to handle values
/// outside the LUT gamut (port of `extrapolate3DArray`).
fn extrapolate_3d_array(array: &Lut3DArray) -> Vec<f32> {
    let dim = array.length();
    let new_dim = dim + 2;
    // An identity of new_dim^3 points, entirely overwritten below.
    let mut new_array = Lut3DArray {
        length: new_dim,
        values: vec![0.0; new_dim * new_dim * new_dim * 3],
    };

    // Copy the center values.
    for i in 0..dim {
        for j in 0..dim {
            for k in 0..dim {
                new_array.set_rgb(i + 1, j + 1, k + 1, array.rgb(i, j, k));
            }
        }
    }

    const CENTER: f32 = 0.5;
    const SCALE: f32 = 4.0;
    let ends = [0, dim - 1];
    let ext = |i: usize| if i == 0 { 0 } else { dim + 1 };

    // Extrapolate faces.
    for i in 0..dim {
        for j in 0..dim {
            for &k in &ends {
                new_array.set_rgb(
                    i + 1,
                    j + 1,
                    ext(k),
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }
    for i in 0..dim {
        for &j in &ends {
            for k in 0..dim {
                new_array.set_rgb(
                    i + 1,
                    ext(j),
                    k + 1,
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }
    for &i in &ends {
        for j in 0..dim {
            for k in 0..dim {
                new_array.set_rgb(
                    ext(i),
                    j + 1,
                    k + 1,
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }

    // Extrapolate edges.
    for &i in &ends {
        for &j in &ends {
            for k in 0..dim {
                new_array.set_rgb(
                    ext(i),
                    ext(j),
                    k + 1,
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }
    for i in 0..dim {
        for &j in &ends {
            for &k in &ends {
                new_array.set_rgb(
                    i + 1,
                    ext(j),
                    ext(k),
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }
    for &i in &ends {
        for j in 0..dim {
            for &k in &ends {
                new_array.set_rgb(
                    ext(i),
                    j + 1,
                    ext(k),
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }

    // Extrapolate corners.
    for &i in &ends {
        for &j in &ends {
            for &k in &ends {
                new_array.set_rgb(
                    ext(i),
                    ext(j),
                    ext(k),
                    extrapolate(array.rgb(i, j, k), CENTER, SCALE),
                );
            }
        }
    }

    new_array.values
}

/// Exact inverse 3D LUT renderer (port of `InvLut3DRenderer`). The inverse
/// is the exact inverse of the tetrahedral interpolation.
#[derive(Debug)]
pub(crate) struct InverseRenderer {
    /// Output scaling for r, g and b.
    scale: f32,
    /// Range tree allowing fast range queries of the LUT.
    tree: RangeTree,
    /// Extrapolated 3D LUT values.
    grvec: Vec<f32>,
}

impl InverseRenderer {
    fn new(lut: &Lut3DOpData) -> Self {
        let grvec = extrapolate_3d_array(lut.array());
        // Extrapolation adds 2.
        let dim = lut.array().length() + 2;
        let tree = RangeTree::new(&grvec, dim);
        // Converts from index units to the unextrapolated LUT domain.
        let scale = 1.0f32 / (dim - 3) as f32;
        Self { scale, tree, grvec }
    }

    fn apply(&self, pixels: &mut [Pixel]) {
        let gsz = &self.tree.gsz;
        let max_dim = (gsz[0] - 3) as f32; // unextrapolated max
        let chans = self.tree.chans;
        let depth = self.tree.depth;
        let levels = &self.tree.levels;
        let base_inds = &self.tree.base_inds;

        let mut offs = [gsz[2] * gsz[1], gsz[2], 1];

        const OPS_LIST: [i64; 8] = [0, 0, 1, 1, 1, 1, 1, 1];
        const ENTERING_LIST: [usize; 8] = [2, 1, 0, 2, 0, 2, 0, 2];
        const NEW_VERTS: [usize; 24] = [
            1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 0,
        ];
        const PATH_LIST: [usize; 24] = [
            0, 0, 0, 0, 0, 0, 0, 1, 2, 1, 0, 2, 1, 2, 0, 2, 1, 0, 2, 0, 1, 0, 2, 1,
        ];
        const PATH_ORDER: [usize; 3] = [1, 0, 2];
        let mut new_vert_list = [0usize; 8];
        for i in 0..8 {
            // Must happen before * chans.
            new_vert_list[i] = NEW_VERTS[i * 3] * offs[0]
                + NEW_VERTS[i * 3 + 1] * offs[1]
                + NEW_VERTS[i * 3 + 2] * offs[2];
        }
        for o in offs.iter_mut() {
            *o *= chans;
        }

        let mut current_child = [0usize; MAX_LEVELS];
        let mut current_num_children = [1usize; MAX_LEVELS];
        let mut current_child_ind = [0usize; MAX_LEVELS];

        if depth == 0 || depth > MAX_LEVELS || levels.is_empty() {
            return;
        }
        let depthm1 = depth as i64 - 1;

        for p in pixels.iter_mut() {
            // Although the inverse LUT has been extrapolated, it may not be
            // enough to cover an HDR float image, so need to clamp.
            const IN_MAX: f32 = 1.0;
            let r = clamp_ocio(p[0], 0.0, IN_MAX);
            let g = clamp_ocio(p[1], 0.0, IN_MAX);
            let b = clamp_ocio(p[2], 0.0, IN_MAX);

            let mut base_indx = [0usize; 3];

            current_num_children[0] = levels[0].child0offsets.len();
            current_child[0] = 0;
            current_child_ind[0] = 0;

            // If no result is found, return 0.
            let mut result = [0f32; 3];

            let mut level: i64 = 0;
            let mut out = [p[0], p[1], p[2]];
            while level >= 0 {
                while current_child[level as usize] < current_num_children[level as usize] {
                    let lv = level as usize;
                    let node = current_child_ind[lv];
                    let lvl = &levels[lv];
                    let in_range = r >= lvl.min_vals[node * chans]
                        && g >= lvl.min_vals[node * chans + 1]
                        && b >= lvl.min_vals[node * chans + 2]
                        && r <= lvl.max_vals[node * chans]
                        && g <= lvl.max_vals[node * chans + 1]
                        && b <= lvl.max_vals[node * chans + 2];
                    current_child[lv] += 1;
                    current_child_ind[lv] += 1;

                    if in_range {
                        if level == depthm1 {
                            base_indx[..chans].copy_from_slice(&base_inds[node].inds[..chans]);
                            let fxval = [r, g, b];
                            let valid = invert_hypercube(
                                3,
                                &mut result,
                                &self.grvec,
                                &offs,
                                &fxval,
                                &base_indx,
                                &OPS_LIST,
                                &ENTERING_LIST,
                                &new_vert_list,
                                &PATH_LIST,
                                &PATH_ORDER,
                            );
                            if valid {
                                level = 0; // to exit the outer loop
                                break;
                            }
                        } else {
                            let new_level = lv + 1;
                            current_num_children[new_level] = lvl.num_children[node];
                            current_child_ind[new_level] = lvl.child0offsets[node];
                            level = new_level as i64;
                            current_child[new_level] = 0;
                        }
                    }
                }
                level -= 1;

                // Need to subtract 1 since the indices include the extrapolation.
                out = [
                    clamp_ocio(result[0] - 1.0, 0.0, max_dim) * self.scale,
                    clamp_ocio(result[1] - 1.0, 0.0, max_dim) * self.scale,
                    clamp_ocio(result[2] - 1.0, 0.0, max_dim) * self.scale,
                ];
            }
            p[0] = out[0];
            p[1] = out[1];
            p[2] = out[2];
            // Alpha is unchanged.
        }
    }
}

/// A 3D LUT CPU renderer (32f in, 32f out).
#[derive(Debug)]
pub(crate) enum Lut3DRenderer {
    Forward(ForwardRenderer),
    Inverse(Box<InverseRenderer>),
}

impl Lut3DRenderer {
    /// Build the renderer of a structurally valid LUT.
    pub(crate) fn new(lut: &Lut3DOpData) -> Self {
        match lut.direction() {
            TransformDirection::Forward => {
                let tetra = lut.concrete_interpolation() == Interpolation::Tetrahedral;
                Lut3DRenderer::Forward(ForwardRenderer::new(lut, tetra))
            }
            TransformDirection::Inverse => {
                Lut3DRenderer::Inverse(Box::new(InverseRenderer::new(lut)))
            }
        }
    }

    pub(crate) fn apply(&self, pixels: &mut [Pixel]) {
        match self {
            Lut3DRenderer::Forward(r) => r.apply(pixels),
            Lut3DRenderer::Inverse(r) => r.apply(pixels),
        }
    }
}
