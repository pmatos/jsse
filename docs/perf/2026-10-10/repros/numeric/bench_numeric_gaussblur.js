// Theme: numeric. Reduction of JetStream SeaMonster/gaussian-blur.js
// (nodeca/glur IIR convolution): ~40 function-local float/int vars, Uint32Array
// reads, Float32Array coefficient reads and tmp-line writes, all inside two
// nested counted loops of one big function. Same source as the benchmark,
// image scaled to 320x180 and one blur pass.
// Measured (buildbox3, 2026-10-10, jsse HEAD 2b621c5 / boa 0.22.0, median of 5): jsse 2.83 s, boa 1.12 s, ratio 2.53x, jsse --no-bytecode 5.43 s.
var a0, a1, a2, a3, b1, b2, left_corner, right_corner;

function gaussCoef(sigma) {
  if (sigma < 0.5) {
    sigma = 0.5;
  }
  var a = Math.exp(0.726 * 0.726) / sigma,
      g1 = Math.exp(-a),
      g2 = Math.exp(-2 * a),
      k = (1 - g1) * (1 - g1) / (1 + 2 * a * g1 - g2);
  a0 = k;
  a1 = k * (a - 1) * g1;
  a2 = k * (a + 1) * g1;
  a3 = -k * g2;
  b1 = 2 * g1;
  b2 = -g2;
  left_corner = (a0 + a1) / (1 - b1 - b2);
  right_corner = (a2 + a3) / (1 - b1 - b2);
  return new Float32Array([ a0, a1, a2, a3, b1, b2, left_corner, right_corner ]);
}

function convolveRGBA(src, out, line, coeff, width, height) {
  var rgba;
  var prev_src_r, prev_src_g, prev_src_b, prev_src_a;
  var curr_src_r, curr_src_g, curr_src_b, curr_src_a;
  var curr_out_r, curr_out_g, curr_out_b, curr_out_a;
  var prev_out_r, prev_out_g, prev_out_b, prev_out_a;
  var prev_prev_out_r, prev_prev_out_g, prev_prev_out_b, prev_prev_out_a;
  var src_index, out_index, line_index;
  var i, j;
  var coeff_a0, coeff_a1, coeff_b1, coeff_b2;

  for (i = 0; i < height; i++) {
    src_index = i * width;
    out_index = i;
    line_index = 0;
    rgba = src[src_index];
    prev_src_r = rgba & 0xff;
    prev_src_g = (rgba >> 8) & 0xff;
    prev_src_b = (rgba >> 16) & 0xff;
    prev_src_a = (rgba >> 24) & 0xff;
    prev_prev_out_r = prev_src_r * coeff[6];
    prev_prev_out_g = prev_src_g * coeff[6];
    prev_prev_out_b = prev_src_b * coeff[6];
    prev_prev_out_a = prev_src_a * coeff[6];
    prev_out_r = prev_prev_out_r;
    prev_out_g = prev_prev_out_g;
    prev_out_b = prev_prev_out_b;
    prev_out_a = prev_prev_out_a;
    coeff_a0 = coeff[0];
    coeff_a1 = coeff[1];
    coeff_b1 = coeff[4];
    coeff_b2 = coeff[5];

    for (j = 0; j < width; j++) {
      rgba = src[src_index];
      curr_src_r = rgba & 0xff;
      curr_src_g = (rgba >> 8) & 0xff;
      curr_src_b = (rgba >> 16) & 0xff;
      curr_src_a = (rgba >> 24) & 0xff;
      curr_out_r = curr_src_r * coeff_a0 + prev_src_r * coeff_a1 + prev_out_r * coeff_b1 + prev_prev_out_r * coeff_b2;
      curr_out_g = curr_src_g * coeff_a0 + prev_src_g * coeff_a1 + prev_out_g * coeff_b1 + prev_prev_out_g * coeff_b2;
      curr_out_b = curr_src_b * coeff_a0 + prev_src_b * coeff_a1 + prev_out_b * coeff_b1 + prev_prev_out_b * coeff_b2;
      curr_out_a = curr_src_a * coeff_a0 + prev_src_a * coeff_a1 + prev_out_a * coeff_b1 + prev_prev_out_a * coeff_b2;
      prev_prev_out_r = prev_out_r;
      prev_prev_out_g = prev_out_g;
      prev_prev_out_b = prev_out_b;
      prev_prev_out_a = prev_out_a;
      prev_out_r = curr_out_r;
      prev_out_g = curr_out_g;
      prev_out_b = curr_out_b;
      prev_out_a = curr_out_a;
      prev_src_r = curr_src_r;
      prev_src_g = curr_src_g;
      prev_src_b = curr_src_b;
      prev_src_a = curr_src_a;
      line[line_index] = prev_out_r;
      line[line_index + 1] = prev_out_g;
      line[line_index + 2] = prev_out_b;
      line[line_index + 3] = prev_out_a;
      line_index += 4;
      src_index++;
    }

    src_index--;
    line_index -= 4;
    out_index += height * (width - 1);
    rgba = src[src_index];
    prev_src_r = rgba & 0xff;
    prev_src_g = (rgba >> 8) & 0xff;
    prev_src_b = (rgba >> 16) & 0xff;
    prev_src_a = (rgba >> 24) & 0xff;
    prev_prev_out_r = prev_src_r * coeff[7];
    prev_prev_out_g = prev_src_g * coeff[7];
    prev_prev_out_b = prev_src_b * coeff[7];
    prev_prev_out_a = prev_src_a * coeff[7];
    prev_out_r = prev_prev_out_r;
    prev_out_g = prev_prev_out_g;
    prev_out_b = prev_prev_out_b;
    prev_out_a = prev_prev_out_a;
    curr_src_r = prev_src_r;
    curr_src_g = prev_src_g;
    curr_src_b = prev_src_b;
    curr_src_a = prev_src_a;
    coeff_a0 = coeff[2];
    coeff_a1 = coeff[3];

    for (j = width - 1; j >= 0; j--) {
      curr_out_r = curr_src_r * coeff_a0 + prev_src_r * coeff_a1 + prev_out_r * coeff_b1 + prev_prev_out_r * coeff_b2;
      curr_out_g = curr_src_g * coeff_a0 + prev_src_g * coeff_a1 + prev_out_g * coeff_b1 + prev_prev_out_g * coeff_b2;
      curr_out_b = curr_src_b * coeff_a0 + prev_src_b * coeff_a1 + prev_out_b * coeff_b1 + prev_prev_out_b * coeff_b2;
      curr_out_a = curr_src_a * coeff_a0 + prev_src_a * coeff_a1 + prev_out_a * coeff_b1 + prev_prev_out_a * coeff_b2;
      prev_prev_out_r = prev_out_r;
      prev_prev_out_g = prev_out_g;
      prev_prev_out_b = prev_out_b;
      prev_prev_out_a = prev_out_a;
      prev_out_r = curr_out_r;
      prev_out_g = curr_out_g;
      prev_out_b = curr_out_b;
      prev_out_a = curr_out_a;
      prev_src_r = curr_src_r;
      prev_src_g = curr_src_g;
      prev_src_b = curr_src_b;
      prev_src_a = curr_src_a;
      rgba = src[src_index];
      curr_src_r = rgba & 0xff;
      curr_src_g = (rgba >> 8) & 0xff;
      curr_src_b = (rgba >> 16) & 0xff;
      curr_src_a = (rgba >> 24) & 0xff;
      rgba = ((line[line_index] + prev_out_r) << 0) +
        ((line[line_index + 1] + prev_out_g) << 8) +
        ((line[line_index + 2] + prev_out_b) << 16) +
        ((line[line_index + 3] + prev_out_a) << 24);
      out[out_index] = rgba;
      src_index--;
      line_index -= 4;
      out_index -= height;
    }
  }
}

function blurRGBA(src32, width, height, radius) {
  var out = new Uint32Array(src32.length),
      tmp_line = new Float32Array(Math.max(width, height) * 4);
  var coeff = gaussCoef(radius);
  convolveRGBA(src32, out, tmp_line, coeff, width, height);
  convolveRGBA(out, src32, tmp_line, coeff, height, width);
}

var width = 320, height = 180;
var buffer = new Uint32Array(width * height);
var seed = 49734321;
for (var i = 0; i < buffer.length; ++i) {
  seed = (seed * 1664525 + 1013904223) >>> 0;
  buffer[i] = seed & 0xfffffff;
}
blurRGBA(buffer, width, height, 15);
var sum = 0;
for (var k = 0; k < buffer.length; k++) sum = (sum + buffer[k]) >>> 0;
console.log("gaussblur", sum);
