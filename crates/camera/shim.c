/* Lets the binary run on older Linux systems (glibc 2.34+): routes the
   C23 number-parsing entry points to the classic ones. No headers on purpose. */
extern int vsscanf(const char *, const char *, __builtin_va_list) __asm__("vsscanf");
extern long strtol(const char *, char **, int) __asm__("strtol");
extern unsigned long strtoul(const char *, char **, int) __asm__("strtoul");
int __isoc23_sscanf(const char *s, const char *f, ...) {
    __builtin_va_list a; __builtin_va_start(a, f);
    int r = vsscanf(s, f, a); __builtin_va_end(a); return r;
}
long __isoc23_strtol(const char *s, char **e, int b) { return strtol(s, e, b); }
unsigned long __isoc23_strtoul(const char *s, char **e, int b) { return strtoul(s, e, b); }
