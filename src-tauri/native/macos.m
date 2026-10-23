#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#include <stdlib.h>
#include <string.h>

static char *json(id value) {
    NSData *data = [NSJSONSerialization dataWithJSONObject:value options:NSJSONWritingFragmentsAllowed error:nil];
    if (!data) return strdup("{\"error\":\"Could not encode native response\"}");
    return strndup(data.bytes, data.length);
}

char *mc_trash(const char *path) {
    @autoreleasepool {
        NSURL *source = [NSURL fileURLWithPath:@(path)];
        NSURL *destination = nil;
        NSError *error = nil;
        BOOL moved = [[NSFileManager defaultManager] trashItemAtURL:source resultingItemURL:&destination error:&error];
        if (!moved) return json(@{@"error": error.localizedDescription ?: @"Could not move item to Trash"});
        return json(@{@"path": destination.path ?: @""});
    }
}

char *mc_pick_folder(void) {
    @autoreleasepool {
        NSOpenPanel *panel = [NSOpenPanel openPanel];
        panel.canChooseDirectories = YES;
        panel.canChooseFiles = NO;
        panel.allowsMultipleSelection = NO;
        panel.canCreateDirectories = NO;
        panel.prompt = @"Add project folder";
        panel.message = @"Choose a project or a folder containing projects to scan for node_modules. Cancel to continue without adding a project folder.";
        if ([panel runModal] != NSModalResponseOK) return json(@{@"path": @""});
        return json(@{@"path": panel.URL.path ?: @""});
    }
}

char *mc_app_info(const char *path) {
    @autoreleasepool {
        NSDictionary *info = [NSDictionary dictionaryWithContentsOfURL:[NSURL fileURLWithPath:[@(path) stringByAppendingPathComponent:@"Contents/Info.plist"]] error:nil];
        id identifier = info[@"CFBundleIdentifier"];
        id bundleName = info[@"CFBundleName"];
        id name = info[@"CFBundleDisplayName"];
        if (![name isKindOfClass:NSString.class] || ![name length]) name = bundleName;
        id version = info[@"CFBundleShortVersionString"];
        return json(@{
            @"bundleId": [identifier isKindOfClass:NSString.class] ? identifier : @"",
            @"name": [name isKindOfClass:NSString.class] ? name : @"",
            @"bundleName": [bundleName isKindOfClass:NSString.class] ? bundleName : @"",
            @"version": [version isKindOfClass:NSString.class] ? version : @""
        });
    }
}

char *mc_app_for_bundle(const char *identifier) {
    @autoreleasepool {
        NSURL *url = [[NSWorkspace sharedWorkspace] URLForApplicationWithBundleIdentifier:@(identifier)];
        return json(@{@"path": url.path ?: @""});
    }
}

int mc_is_running(const char *identifier) {
    @autoreleasepool {
        return [NSRunningApplication runningApplicationsWithBundleIdentifier:@(identifier)].count > 0;
    }
}

char *mc_quit_app(const char *identifier) {
    @autoreleasepool {
        NSString *bundle = @(identifier);
        if ([@[@"com.maccleaner.desktop", @"com.apple.finder", @"com.apple.dock", @"com.apple.loginwindow", @"com.apple.SystemUIServer"] containsObject:bundle]) {
            return json(@{@"error": @"This app cannot be quit from Mac Cleaner. Deselect its files to continue."});
        }
        NSUInteger requested = 0;
        for (NSRunningApplication *app in [NSRunningApplication runningApplicationsWithBundleIdentifier:bundle]) {
            if (app.processIdentifier == NSProcessInfo.processInfo.processIdentifier) {
                return json(@{@"error": @"Mac Cleaner cannot quit itself during cleanup."});
            }
            if ([app terminate]) requested++;
            else if (!app.terminated) {
                return json(@{@"error": [NSString stringWithFormat:@"Could not ask %@ to quit. Quit it from its own menu, then retry.", app.localizedName ?: bundle]});
            }
        }
        return json(@{@"requested": @(requested)});
    }
}

int mc_reveal(const char *path) {
    @autoreleasepool {
        [[NSWorkspace sharedWorkspace] activateFileViewerSelectingURLs:@[[NSURL fileURLWithPath:@(path)]]];
        return 1;
    }
}

int mc_open_trash(void) {
    @autoreleasepool {
        return [[NSWorkspace sharedWorkspace] openURL:[NSURL fileURLWithPath:[NSHomeDirectory() stringByAppendingPathComponent:@".Trash"]]];
    }
}

int mc_open_update(void) {
    @autoreleasepool {
        return [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:@"https://github.com/stormznet/MacOS-Cleaner/releases/latest/download/MacCleaner-arm64.dmg"]];
    }
}

int mc_open_privacy_settings(int full_disk_access) {
    @autoreleasepool {
        NSURL *bundle = NSBundle.mainBundle.bundleURL;
        if (full_disk_access && [bundle.pathExtension isEqualToString:@"app"]) {
            [[NSWorkspace sharedWorkspace] activateFileViewerSelectingURLs:@[bundle]];
        }
        NSString *address = full_disk_access
            ? @"x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles"
            : @"x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders";
        return [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:address]];
    }
}
