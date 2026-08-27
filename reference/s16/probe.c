#include <stdio.h>
#include <math.h>
int main(void){
    float maxval=1.45f, x=-1.45f;
    float a=(maxval-1)/(maxval*maxval);
    float ab=a; ab += ab*2.4e-7f;
    printf("a=%.9g a_boost=%.9g\n",(double)a,(double)ab);
    float v1 = x + ab*x*x;                 /* as written, compiler's choice */
    float t  = ab*x; float v2 = x + t*x;   /* strict left-to-right */
    float v3 = x + ab*(x*x);
    float v4 = fmaf(ab*x, x, x);
    printf("as-written   = %.9g\n",(double)v1);
    printf("(a*x)*x      = %.9g\n",(double)v2);
    printf("a*(x*x)      = %.9g\n",(double)v3);
    printf("fma(a*x,x,x) = %.9g\n",(double)v4);
    printf("ours=%.9g libopus=%.9g\n",(double)-0.999999881f,(double)-0.999999940f);
    return 0;
}
