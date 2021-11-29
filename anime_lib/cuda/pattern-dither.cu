// experimental pattern dithering cuda shader. it ain't great but it works?

extern "C" __device__ float clamp(float a, float min, float max) {
    if (a < min) {
        return min;
    } else if (a > max) {
        return max;
    } else {
        return a;
    }
}

// literally just bubblesort 
extern "C" __device__ void sort(float arr[64][4]) {
    for (int step = 0; step < 63; ++step) {
        bool swapped = false;

        for (int i= 0; i < 63 - step; ++i) {
            if (arr[i][3] > arr[i + 1][3]) {
                swapped = true;
                for (int j = 0; j < 3; ++j) {
                    float tmp = arr[i][j];
                    arr[i][j] = arr[i+1][j];
                    arr[i+1][j] = tmp;
                }
            }
        }

        if (!swapped) {
            break;
        }
    }
}

extern "C" __global__ void delta_e(const float* palette, const float* rgb_palette, const float* pixel, float* out, int H, int W, int matrix_size, int* matrix, float multiplier) {
    int row = (blockIdx.y * blockDim.y) + threadIdx.y;
    int col = (blockIdx.x * blockDim.x) + threadIdx.x;

    int offset = (row * W + col) * 3;

    if ((row < H) && (col < W)) {
        float candidates[64][4];

        float acc[3] = {0.0,0.0,0.0};

        float src_r = pixel[offset];
        float src_g = pixel[offset + 1];
        float src_b = pixel[offset + 2];

        int bayer_idx = matrix[
            (row % 8)
            * 8
            + (col % 8)
        ];

        for (int j = 0; j < matrix_size; ++j) {
            float r = clamp(src_r + (acc[0] * multiplier), 0.0, 255.0) / 255.0;
            float g =  clamp(src_g + (acc[1] * multiplier), 0.0, 255.0) / 255.0;
            float b =  clamp(src_b + (acc[2] * multiplier), 0.0, 255.0) / 255.0;
            // printf("FIRST src_r %f src_g %f src_b %f \n r %f g %f b %f\n",src_r,src_g,src_b,r,g,b);

            if (r > 0.04045) {
                r = __powf(((r + 0.055) / 1.055), 2.4);
            } else {
                r = r / 12.92;
            }

            r = r * 100.0;

            if (g > 0.04045) {
                g = __powf(((g + 0.055) / 1.055), 2.4);
            } else {
                g = g / 12.92;
            }

            g = g * 100.0;

            if (b > 0.04045) {
                b = __powf(((b + 0.055) / 1.055), 2.4);
            } else {
                b = b / 12.92;
            }

            b = b * 100.0;

            // printf("src_r %f src_g %f src_b %f\n linearized: r %f g %f b %f\n",src_r,src_g,src_b,r,g,b);


            float x = r * 0.4124 + g * 0.3576 + b * 0.1805;
            float y = r * 0.2166 + g * 0.7152 + b * 0.0722;
            float z = r * 0.0193 + g * 0.1192 + b * 0.9505;

            x = x / 95.047;
            if (x > 0.008856) {
                x = __powf(x, 1.0 / 3.0);
            } else {
                x = (x * 7.787) + 16.0 / 116.0;
            }

            y = y / 100.000;

            if (y > 0.008856) {
                y = __powf(y, 1.0 / 3.0);
            } else {
                y = (y * 7.787) + 16.0 / 116.0;
            }

            z = z / 108.883;

            if (z > 0.008856) {
                z = __powf(z, 1.0 / 3.0);
            } else {
                z = (z * 7.787) + 16.0 / 116.0;
            }

            float l_l = (y * 116.0) - 16.0;
            float l_a = (x - y) * 500.0;
            float l_b = (y - z) * 200.0;

            float least_dist = 10000000000000000000000.0;
            int least_index = 276;

            for (int i = 0; i < 256; ++i) {
                int pal_offset = i * 3;
                float distl = palette[pal_offset] - l_l;
                float dista = palette[pal_offset + 1] - l_a;
                float distb = palette[pal_offset + 2] - l_b;
                float dist = (distl * distl) + (dista * dista) + (distb * distb);
                if (dist < least_dist) {
                    least_dist = dist;
                    least_index = i;
                }
            }
                        
            int pal_offset = least_index * 3;
            float p_r = rgb_palette[pal_offset];
            float p_g = rgb_palette[pal_offset + 1];
            float p_b = rgb_palette[pal_offset + 2];

            candidates[j][0] = p_r;
            candidates[j][1] = p_g;
            candidates[j][2] = p_b;

            candidates[j][3] = (p_r * 299.0 + p_g * 587.0 + p_b * 114.0) / (255.0 * 1000.0);

            acc[0] = acc[0] + (src_r - p_r);
            acc[1] = acc[1] + (src_g - p_g);
            acc[2] = acc[2] + (src_b - p_b);
        }

        sort(candidates);

        out[offset] = candidates[bayer_idx][0];
        out[offset + 1] = candidates[bayer_idx][1];
        out[offset + 2] = candidates[bayer_idx][2];
    }
}