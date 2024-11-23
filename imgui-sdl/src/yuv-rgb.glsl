/** A fragment shader to convert YUV420P to RGB.
  * Input textures Y - is a block of size w*h. 
  * texture U is of size w/2*h/2.
  * texture V is of size w/2*h/2. 
  * In this case, the layout looks like the following :
  * __________
  * |        |
  * |   Y    | size = w*h
  * |        |
  * |________|
  * |____U___|size = w*(h/4)
  * |____V___|size = w*(h/4)
  */
precision highp float;
varying vec2 vTextureCoord;
uniform sampler2D sTextureY;
uniform sampler2D sTextureU;
uniform sampler2D sTextureV;
void main (void) {
    float r, g, b, y, u, v;
    y = texture2D(sTextureY, vTextureCoord).r;
    u = texture2D(sTextureU, vTextureCoord).r;
    v = texture2D(sTextureV, vTextureCoord).r;

    y = 1.1643*(y-0.0625);
    u = u-0.5;
    v = v-0.5;

    r = y+1.5958*v;
    g = y-0.39173*u-0.81290*v;
    b = y+2.017*u;
    gl_FragColor = vec4(r, g, b, 1.0);
}
