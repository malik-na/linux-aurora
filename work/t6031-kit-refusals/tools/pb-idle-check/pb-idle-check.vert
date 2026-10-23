#version 450
// SPDX-License-Identifier: MIT
// Triangle i: a pseudo-random centre over the target and a fixed size, so that each triangle
// covers many tiles and the tiler's per-tile lists (the parameter buffer) grow with the count.
layout(push_constant) uniform P { uint seed; float size; } p;
layout(location = 0) out vec3 colour;

uint hash(uint x)
{
	x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
	return x;
}

void main()
{
	uint tri = uint(gl_VertexIndex) / 3u, corner = uint(gl_VertexIndex) % 3u;
	uint h = hash(tri ^ (p.seed * 0x9e3779b9u));
	vec2 centre = vec2(float(h & 0xffffu), float(h >> 16)) / 65535.0 * 2.0 - 1.0;
	vec2 offs[3] = vec2[3](vec2(-1.0, -0.6), vec2(1.0, -0.6), vec2(0.0, 1.0));
	gl_Position = vec4(centre + offs[corner] * p.size, float(hash(h) & 0xffu) / 256.0, 1.0);
	colour = vec3(float(h & 0xffu), float((h >> 8) & 0xffu), float((h >> 24) | 1u)) / 255.0;
}
