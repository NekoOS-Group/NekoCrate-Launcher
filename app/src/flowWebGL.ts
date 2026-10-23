import type { FlowGraph } from "./flowGraph.ts";
import type { HexFlow } from "./hexFlow.ts";
import { createFlowPaintState, type FlowPalette } from "./flowPainter.ts";
import { createFlowBatchGeometry } from "./flowGeometry.ts";

const vertex = `
attribute vec4 geometry0,geometry1,paint;
attribute vec3 geometry2;
attribute vec2 tileOffset;
uniform vec2 viewport,drift;
uniform vec3 cyan,blue,aqua;
varying highp vec4 boundary0,boundary1,color;
varying highp vec2 tilePoint;
varying highp float line;
void main() {
  line=geometry2.z;
  if(line>0.5) {
    vec2 delta=geometry0.zw-geometry0.xy;
    float segmentLength=length(delta);
    vec2 direction=delta/segmentLength,normal=vec2(-direction.y,direction.x);
    vec2 halfSize=vec2(segmentLength,paint.x)*0.5;
    vec2 local=geometry1.xy*(halfSize+1.0);
    boundary0=vec4(local,halfSize);boundary1=vec4(0.0);
    tilePoint=(geometry0.xy+geometry0.zw)*0.5+direction*local.x+normal*local.y;
    color=vec4(blue,paint.z);
  } else {
    boundary0=vec4(geometry0.zw,geometry1.xy);
    boundary1=vec4(geometry1.zw,geometry2.xy);
    tilePoint=geometry0.xy;
    color=vec4(mix(mix(aqua,blue,paint.y),cyan,paint.x),paint.z);
  }
  vec2 screen=(tilePoint+tileOffset+drift)/viewport;
  gl_Position=vec4(screen.x*2.0-1.0,1.0-screen.y*2.0,0.0,1.0);
}`;
const fragment = `
#extension GL_OES_standard_derivatives : enable
precision highp float;
uniform vec2 period,pixelScale;
varying highp vec4 boundary0,boundary1,color;
varying highp vec2 tilePoint;
varying highp float line;
// Pixel-area coverage keeps subpixel-width lines smooth without MSAA buffers.
float halfPlane(float d,vec2 slope) {
  float a=max(abs(slope.x),abs(slope.y)),b=min(abs(slope.x),abs(slope.y));
  if(b<0.00001)return clamp(0.5+d/max(a,0.00001),0.0,1.0);
  float t=d+(a+b)*0.5;
  if(t<=0.0)return 0.0;
  if(t>=a+b)return 1.0;
  float u=max(t-a,0.0),v=max(t-b,0.0);
  return clamp((t*t-u*u-v*v)/(2.0*a*b),0.0,1.0);
}
void main() {
  float coverage;
  // The kind is constant across every primitive, including helper lanes.
  if(line>0.5) {
    vec2 dx=dFdx(boundary0.xy),dy=dFdy(boundary0.xy);
    float x=halfPlane(boundary0.z+boundary0.x,vec2(dx.x,dy.x))+halfPlane(boundary0.z-boundary0.x,vec2(dx.x,dy.x))-1.0;
    float y=halfPlane(boundary0.w+boundary0.y,vec2(dx.y,dy.y))+halfPlane(boundary0.w-boundary0.y,vec2(dx.y,dy.y))-1.0;
    coverage=max(min(x,y),0.0);
  } else {
    vec4 d=min(boundary0,boundary1);
    float nearest=min(min(d.x,d.y),min(d.z,d.w));
    coverage=halfPlane(nearest,vec2(dFdx(nearest),dFdy(nearest)));
  }
  vec2 clip=clamp(tilePoint*pixelScale+0.5,0.0,1.0)+clamp((period-tilePoint)*pixelScale+0.5,0.0,1.0)-1.0;
  float alpha=color.a*coverage*max(clip.x,0.0)*max(clip.y,0.0);
  gl_FragColor=vec4(color.rgb*alpha,alpha);
}`;

/** One dedicated context, one resident ordered mesh, one paint-buffer upload
 * and one instanced draw per frame. Instancing a whole tile preserves original
 * source-over order at seams. Unsupported instancing uses one draw per tile.
 */
export function createFlowWebGL(
  canvas: HTMLCanvasElement,
  graph: FlowGraph,
  palette: FlowPalette,
) {
  const gl = canvas.getContext("webgl", {
    alpha: true,
    antialias: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: true,
    powerPreference: "low-power",
  });
  if (
    !gl ||
    !gl.getExtension("OES_standard_derivatives") ||
    !gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER, gl.HIGH_FLOAT)?.precision
  )
    throw new Error("Required WebGL precision/derivatives are unavailable");
  const geometry = createFlowBatchGeometry(graph);
  const state = createFlowPaintState(graph.edges.length, graph.cells.length);
  const paint = new Float32Array(geometry.ids.length * 4);
  let width = 0,
    height = 0,
    ratio = 0;
  let program: WebGLProgram,
    paintBuffer: WebGLBuffer,
    offsetBuffer: WebGLBuffer;
  let instancing: ANGLE_instanced_arrays | null;
  let offsetAttribute: number, drift: WebGLUniformLocation | null;
  let copies = new Float32Array();
  const buffers: WebGLBuffer[] = [];
  function buffer(data: Float32Array, dynamic = false) {
    const b = gl!.createBuffer()!;
    buffers.push(b);
    gl!.bindBuffer(gl!.ARRAY_BUFFER, b);
    gl!.bufferData(
      gl!.ARRAY_BUFFER,
      data,
      dynamic ? gl!.DYNAMIC_DRAW : gl!.STATIC_DRAW,
    );
    return b;
  }
  function attribute(
    name: string,
    size: number,
    stride: number,
    offset: number,
  ) {
    const location = gl!.getAttribLocation(program, name);
    gl!.enableVertexAttribArray(location);
    gl!.vertexAttribPointer(location, size, gl!.FLOAT, false, stride, offset);
    return location;
  }
  function viewport() {
    if (!width || !height) return;
    gl!.viewport(0, 0, canvas.width, canvas.height);
    gl!.uniform2f(gl!.getUniformLocation(program, "viewport"), width, height);
    gl!.uniform2f(
      gl!.getUniformLocation(program, "pixelScale"),
      canvas.width / width,
      canvas.height / height,
    );
    const columns = Math.ceil(width / graph.width) + 1,
      rows = Math.ceil(height / graph.height) + 1;
    copies = new Float32Array(columns * rows * 2);
    for (let row = 0; row < rows; row++)
      for (let col = 0; col < columns; col++) {
        const i = (row * columns + col) * 2;
        copies[i] = col * graph.width;
        copies[i + 1] = row * graph.height;
      }
    if (instancing) {
      gl!.bindBuffer(gl!.ARRAY_BUFFER, offsetBuffer);
      gl!.bufferData(gl!.ARRAY_BUFFER, copies, gl!.STATIC_DRAW);
    }
  }
  function initialize() {
    gl!.getExtension("OES_standard_derivatives");
    instancing = gl!.getExtension("ANGLE_instanced_arrays");
    buffers.length = 0;
    program = gl!.createProgram()!;
    const shaders = [
      gl!.createShader(gl!.VERTEX_SHADER)!,
      gl!.createShader(gl!.FRAGMENT_SHADER)!,
    ];
    for (let i = 0; i < 2; i++) {
      gl!.shaderSource(shaders[i]!, [vertex, fragment][i]!);
      gl!.compileShader(shaders[i]!);
      gl!.attachShader(program, shaders[i]!);
    }
    gl!.bindAttribLocation(program, 0, "geometry0");
    gl!.linkProgram(program);
    const linked = gl!.getProgramParameter(program, gl!.LINK_STATUS);
    const error = linked
      ? ""
      : shaders.map((s) => gl!.getShaderInfoLog(s)).join("\n") +
        gl!.getProgramInfoLog(program);
    for (const shader of shaders) gl!.deleteShader(shader);
    if (!linked) throw new Error(error);
    gl!.useProgram(program);
    for (const [name, rgb] of Object.entries(palette))
      gl!.uniform3f(
        gl!.getUniformLocation(program, name),
        rgb[0] / 255,
        rgb[1] / 255,
        rgb[2] / 255,
      );
    gl!.uniform2f(
      gl!.getUniformLocation(program, "period"),
      graph.width,
      graph.height,
    );
    drift = gl!.getUniformLocation(program, "drift");
    buffer(geometry.vertices);
    attribute("geometry0", 4, 44, 0);
    attribute("geometry1", 4, 44, 16);
    attribute("geometry2", 3, 44, 32);
    paintBuffer = buffer(paint, true);
    attribute("paint", 4, 16, 0);
    offsetBuffer = buffer(copies);
    offsetAttribute = gl!.getAttribLocation(program, "tileOffset");
    if (instancing) {
      attribute("tileOffset", 2, 8, 0);
      instancing.vertexAttribDivisorANGLE(offsetAttribute, 1);
    } else {
      gl!.disableVertexAttribArray(offsetAttribute);
      gl!.vertexAttrib2f(offsetAttribute, 0, 0);
    }
    gl!.disable(gl!.DEPTH_TEST);
    gl!.disable(gl!.CULL_FACE);
    gl!.disable(gl!.DITHER);
    gl!.enable(gl!.BLEND);
    gl!.blendFunc(gl!.ONE, gl!.ONE_MINUS_SRC_ALPHA);
    gl!.clearColor(0, 0, 0, 0);
    viewport();
  }
  function release() {
    for (const b of buffers) gl!.deleteBuffer(b);
    if (program) gl!.deleteProgram(program);
  }
  try {
    initialize();
  } catch (error) {
    release();
    throw error;
  }
  return {
    resize(w: number, h: number, r: number) {
      if (w === width && h === height && r === ratio) return false;
      width = Math.max(1, w);
      height = Math.max(1, h);
      ratio = Math.max(1, r);
      canvas.width = Math.ceil(width * ratio);
      canvas.height = Math.ceil(height * ratio);
      viewport();
      return true;
    },
    paint(flow: HexFlow, seconds: number, still = false) {
      if (!width || !height || gl.isContextLost()) return;
      state.update(flow, still);
      for (let i = 0; i < geometry.ids.length; i++) {
        const id = geometry.ids[i]!,
          at = i * 4;
        if (i < geometry.fillCount) {
          paint[at] = id < 0 ? 0 : state.cyanMix[id]! / 200;
          paint[at + 1] =
            id < 0
              ? 0
              : state.blueMix[id]! < 0
                ? 0.5
                : state.blueMix[id]! / 200;
          paint[at + 2] = id < 0 ? 0.025 : state.opacity[id]!;
        } else {
          paint[at] = state.width[id]!;
          paint[at + 2] = state.intensity[id]!;
        }
      }
      gl.bindBuffer(gl.ARRAY_BUFFER, paintBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, paint);
      const phase = still ? 0 : ((seconds * 10) / graph.width) % 1;
      gl.uniform2f(drift, -phase * graph.width, -phase * graph.height);
      gl.clear(gl.COLOR_BUFFER_BIT);
      if (instancing)
        instancing.drawArraysInstancedANGLE(
          gl.TRIANGLES,
          0,
          geometry.ids.length,
          copies.length / 2,
        );
      else
        for (let i = 0; i < copies.length; i += 2) {
          gl.vertexAttrib2f(offsetAttribute, copies[i]!, copies[i + 1]!);
          gl.drawArrays(gl.TRIANGLES, 0, geometry.ids.length);
        }
    },
    restore() {
      initialize();
    },
    dispose() {
      release();
      gl.getExtension("WEBGL_lose_context")?.loseContext();
      canvas.width = canvas.height = 0;
    },
  };
}
