#define _GNU_SOURCE
#include <X11/Xlib.h>
#include <X11/Xatom.h>
#include <X11/Xutil.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// Real XMapWindow function pointer
static int (*real_XMapWindow)(Display*, Window) = NULL;

// Check if window belongs to 0fbuf pool by reading WM_CLASS
static int is_pool_window(Display *display, Window w) {
    XClassHint class_hint;
    if (XGetClassHint(display, w, &class_hint)) {
        int is_pool = (class_hint.res_class && strstr(class_hint.res_class, "0fbuf-") != NULL);
        if (class_hint.res_name) XFree(class_hint.res_name);
        if (class_hint.res_class) XFree(class_hint.res_class);
        return is_pool;
    }
    return 0;
}

// Get pool desktop from environment
static unsigned long get_pool_desktop() {
    char *env = getenv("OFBUF_POOL_DESKTOP");
    return env ? atol(env) : 0;
}

// Set window to pool desktop BEFORE mapping
static void prepare_pool_window(Display *display, Window w) {
    // Get atoms
    Atom net_wm_desktop = XInternAtom(display, "_NET_WM_DESKTOP", False);
    Atom net_wm_state = XInternAtom(display, "_NET_WM_STATE", False);
    Atom net_wm_state_skip_taskbar = XInternAtom(display, "_NET_WM_STATE_SKIP_TASKBAR", False);
    Atom net_wm_state_skip_pager = XInternAtom(display, "_NET_WM_STATE_SKIP_PAGER", False);
    Atom cardinal = XInternAtom(display, "CARDINAL", False);
    Atom atom_type = XInternAtom(display, "ATOM", False);

    unsigned long desktop = get_pool_desktop();

    // Set _NET_WM_DESKTOP property BEFORE window is mapped
    XChangeProperty(display, w, net_wm_desktop, cardinal, 32,
                    PropModeReplace, (unsigned char*)&desktop, 1);

    // Set SKIP_TASKBAR and SKIP_PAGER states so window is hidden while pooled
    Atom states[2] = {net_wm_state_skip_taskbar, net_wm_state_skip_pager};
    XChangeProperty(display, w, net_wm_state, atom_type, 32,
                    PropModeReplace, (unsigned char*)states, 2);

    // Set window to be off-screen initially
    XMoveWindow(display, w, -10000, -10000);

    XFlush(display);

    fprintf(stderr, "[0fbuf-preload] Prepared pool window 0x%lx for desktop %lu (hidden from taskbar)\n", w, desktop);
}

// Intercepted XMapWindow
int XMapWindow(Display *display, Window w) {
    // Load real XMapWindow on first call
    if (!real_XMapWindow) {
        real_XMapWindow = dlsym(RTLD_NEXT, "XMapWindow");
        if (!real_XMapWindow) {
            fprintf(stderr, "[0fbuf-preload] ERROR: Could not find real XMapWindow\n");
            return 0;
        }
    }

    // Check if this is a pool window
    if (is_pool_window(display, w)) {
        // Prepare window BEFORE mapping
        prepare_pool_window(display, w);
    }

    // Call real XMapWindow
    return real_XMapWindow(display, w);
}

// Constructor to announce we're loaded
__attribute__((constructor))
static void init() {
    fprintf(stderr, "[0fbuf-preload] Loaded! Intercepting XMapWindow for pool windows.\n");
}
