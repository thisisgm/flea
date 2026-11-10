"""Check the default oracle and the separately declared GPUI replacements."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

BASE = "2faf8d099e12303cfb5a28af4a0c16c851a2cb07"
NATIVE_TAIL_SHA256 = 'adbde831f33b613e7c0bab4e4b82ed2a17b00858d640d11fa28594ca1ba07d4d'
# Filled from the exact base, never an older archive's manifest; a body edited since is repinned here.
ORACLE = {name: {"path": path, "sha256": digest} for name, path, digest in (
    ('cursor', 'tests/ui.sh', 'd15f81b18a714d20411d6ad34818957c9fbf07399e85c8b286ec57fd1001f644'),
    ('scroll', 'tests/ui.sh', '32ed9275187ccfdba7fbb397444e5ffce7d0b1465463d23bbc13406428d2869d'),
    ('scrollbar', 'tests/ui.sh', '5bb29e45c18ec8e7e5e0a60142e5416e2c411f6fea25e4ee4f01b283cfb5dcf9'),
    ('terminal', 'tests/ui.sh', 'ff01f9c6a2e91f0e2a7fb66e8a48b53fb3456f078e3a0f311513323e7e2d2b46'),
    ('open', 'tests/ui.sh', '57d752f608b008e4f5e19be002a7a5f6cd3d5437dc8d299fd5c4127573733c8b'),
    ('rows', 'tests/ui.sh', 'e7afd2c39cc1091fd36b9faa6e54f161637925a1bb7af70f7133cde938e23a17'),
    ('click', 'tests/ui.sh', 'b21588f1a9f256afee764d44e864d05fb6536fb45269be9875dd29f77e25432c'),
    ('clickedge', 'tests/ui.sh', '192ead9e062ab8b952e05e7acaf117467b29af9f5246ed5db4acfa2aa1bc6470'),
    ('ctrlclick', 'tests/ui.sh', '39fff2fe446fd484c3e4cb3017c03297afb23d15155ced60fee5988f93d4b264'),
    ('shiftscroll', 'tests/ui.sh', 'd0877506788c8ba9ef1405bc760ec0ab89d2e9c1ea611c21e4c149f7cb6cd453'),
    ('querypaste', 'tests/ui.sh', '13ef56a617588e507169845b26a5bd4b2d6c03bc03efffe81f1ab552c64d42e1'),
    ('viewrestart', 'tests/ui.sh', 'bf0e07f1095f193236437029144e382e8ce02aeef78bbdca947d4c34b7abe17b'),
    ('dd', 'tests/ui.sh', '502c33ea0c963ed90f8112e54c9f1ab00665afecdddcc15f8bd579634bc7ec29'),
    ('ddclick', 'tests/ui.sh', 'c6c80618703e0f3acab1571ea6b6a5036acc6a7912fd57d65a9e9368f8384346'),
    ('griddrop', 'tests/ui-griddrop.sh', 'f43e96365d70a3d6f066f4bbe28b9baed80f8b58093b58f71bad3c0137ffa63a'),
    ('collide', 'tests/ui.sh', 'c2c3e70f967eaf969fc9c386d1eae9a4b1a952d46901859ba7dafc0cf343c9a2'),
    ('sortrestart', 'tests/ui.sh', '939e1c6109e1fb3dd0ed51cefeab59351d6ad4de43b3de0cb366cf2a20639086'),
    ('foldersort', 'tests/ui.sh', '5092a01a3cbaf6fbc18040827e22cbb3ef80663fadad89f9e930661ec028def2'),
    ('duallaunch', 'tests/ui.sh', 'f25e5eaf1a1daffc4d2c10942e639bd633da45550c5bbd846bd50d5a9cee0874'),
    ('dirsortstale', 'tests/ui-dirsortstale.sh', '7a60d5d687f015a6135fd6265ce5eecd53fafe4f4c76b604e37b1b4c9fd0c36e'),
    ('editplace', 'tests/ui.sh', 'a965e4a574627d3c8f8d350c01322cdf3e628316d22ee1db4970f11fea48e6a6'),
    ('mute', 'tests/ui.sh', '57cd00221bed8cf607d378a86717fb1e3450aa08d6347498e06ffd892175c521'),
    ('placemenu', 'tests/ui.sh', 'c604979f883a9122b3a107b9ef4bd17a67f89602cc2a55ec214aa8f13d6536bf'),
    ('runscript', 'tests/ui.sh', '723ad905856a0fe4cc32b68a045192274ce4fe36df83ae0158be3c72f3a04614'),
    ('unmounted', 'tests/ui-rail.sh', 'f1804220b8f2a056de1da429f31e8f75cb456fe7dcdc0ff66059aa5b89323426'),
    ('sidebar', 'tests/ui-rail.sh', 'fb17653b1db7b9bb67933f804af5325c08fff8d6f13661dcb065325d25bedc9a'),
    ('menu', 'tests/ui.sh', '9186a25751ce26d5102900fd21db9860697749d32ad977c984c94cfdffa1ee9b'),
    ('background', 'tests/ui.sh', '2a77c26859ae26e0646ed126cca5f2346fcd7f141fabf09876ff4270a093806c'),
    ('hidden', 'tests/ui.sh', '03257bbfb34b9f81bf67958c0b0a447d3448f7153cea4368a036e8a66fd542fc'),
    ('xwsettings', 'tests/ui.sh', 'daa74c2c86dde1967f971a0df91bcc6da812c5ab9bc57376aa4592e1572341d8'),
    ('selection', 'tests/ui.sh', '92ce6c995c22a184ecd7f6cccc414e18f79fdd495b9b4fa4d2282f4df7fe8514'),
    ('watch', 'tests/ui.sh', 'f1b13d9cde66e99e07750eb25146e4836c68fb763198f024c341fd70c5899d7c'),
    ('xwwatch', 'tests/ui.sh', 'ab7890703644a55b5a2f9b322258ef71480f7fb49179c793d754703c48bb0d2b'),
    ('reload', 'tests/ui.sh', '77d9def72c8f8b11683651263075485c5ed1055a5e40cdb4e94935d58ea84adb'),
    ('optical', 'tests/ui.sh', '5f624242701bc273f948bf4782b6db27cb7dbc00066c45c02fb0681b8bb789b4'),
    ('select', 'tests/ui.sh', '66ff527815affdaefa4631b51f46a5beb23b54606f93e8e2c151507f4ea45827'),
    ('colour', 'tests/ui.sh', '19100faf9e5061830fcfa570a1106448077f184de2305f0079370c2b48310a31'),
    ('lifted', 'tests/ui.sh', '70de81758b9d3471e94ef8b0445bd96b185cc0d19bd7e3acdeb5ff541617dcfa'),
    ('icons', 'tests/ui.sh', 'f0d4df81e9de32b1fe28952a5ad2faec051a64910abc18fb42dff64d37cffcc2'),
    ('thumbs', 'tests/ui.sh', 'b1cbfe52f701a55b22eb6a3b41c4f622f645bfc8248efff9814762b6097b7902'),
    ('hashcache', 'tests/ui.sh', '9ee9af3781029d195f07f6c514726fe25582d1f5b9bf27def8bd84751f9b3adc'),
    ('stale', 'tests/ui.sh', 'a0219faa75ddca6a27b2195701ebc2356b198acf546a73c37395e1f7954a7bd7'),
    ('nosweep', 'tests/ui.sh', 'd56290975581d51abdc678512bf2c78e932f5818e21e4d9ad221eaf015d388bc'),
    ('oem', 'tests/ui.sh', 'cc3d71e5d51e1da1c0c45ab82cee41066c00167ba673a472cab2666299a6a8e1'),
    ('header', 'tests/ui.sh', '9fc068d948f45a43938769cade19e5b2eae26531be1ce411d782988c9e68daa4'),
    ('columnresize', 'tests/ui.sh', '7dcec85ae9f92cd83e96a80508e73e8837b2788e7e379b54a8f31db9ea8c824b'),
    ('columnautofit', 'tests/ui.sh', 'e5503becdead779a91279d0d345c044324a280f64402d0b1881687a9699efbc3'),
    ('overflow', 'tests/ui.sh', '0143f3a7e25ea80f3293ff8ce7f9d82199ddc67cc3fad1f414f0621bf55f9fc3'),
    ('focus', 'tests/ui.sh', '89e79cfa64b745cb3557fed3dfeda779a20e33070071cfc64eed08cba62df465'),
    ('railpointer', 'tests/ui-railpointer.sh', '72c02075ebeb17d1192e9b436410b0165a8f94db28bbfc07353fd8bcaecb265e'),
    ('dualautohidereveal', 'tests/ui-railpointer.sh', '4df25127480c6f874ba6dd4826e8658046ebac35c68a96a136d2b4278d2a6498'),
    ('preview', 'tests/ui.sh', '0b5047f9fa56866286876df32d13a7a53b1780377045aedb127e149181fcd1d5'),
    ('pdffocus', 'tests/ui-pdf.sh', '16784fd488cdf0954feff7d263af4c69ad061d381fa2377e24ce28b35f3029a7'),
    ('network', 'tests/ui.sh', '5095f896af4b1e26fcac768cdf702fd5e7b50aab029554e35a222eb1994eba87'),
    ('netmark', 'tests/ui.sh', '6f4ccf12c879404b7e1862eb7b60ca334d3d8d91decc80a886404baea33161e8'),
    ('networkauth', 'tests/ui.sh', 'b746ed6a778741b02048108c859a172876a4b786a747a3b8101f8b405d8fbfc5'),
    ('networktimeout', 'tests/ui.sh', 'ad391b4206c25a8a53b6ba5ca16cb9b5bb9b9d5cf4794133792285c5c0951a75'),
    ('gvfs', 'tests/ui.sh', '6f621fe825914b172c01f0d275b7d9c65a6dcc3858ebc44d345af714bdb162cb'),
    ('sharebrowser', 'tests/ui.sh', 'a83b586d31183def10330f3942e6936b5391390af1f88bc73fe8ad844f57198f'),
    ('unmount', 'tests/ui.sh', 'fe45769f83149f0b7351ca3f503a8f8deee03248767605c5033fad328be7062b'),
    ('phones', 'tests/ui.sh', '6e10956870ebbc1743cee06de10ae05cf4f543f24d10979b333357f5c506c39e'),
    ('trasharm', 'tests/ui-trash.sh', 'f385d41e47a826901b76b8bc31f28d156d64be7cda59b946a159e461d13305f7'),
    ('eject', 'tests/ui.sh', 'c5cd7f5a3b8a77262ca7227879c79b555791617eb1cf7bd9cd8abac023d01f2f'),
    ('poweroff', 'tests/ui.sh', 'c13495b30838d2514f5920a24db93cf6a6c3b7ba3a013bc10d41c7a03ee273e7'),
    ('rename', 'tests/ui.sh', '5ffcbc83c936d8a836096b550926d23a27eb1a8d1ca29c7f591c59f2e1f1a10b'),
    ('renamefirst', 'tests/ui.sh', 'a8dfdd1cc684f5dc0d2b69400b9270cad6577883c53f7be559c362d4b94ea6bf'),
    ('renamelife', 'tests/ui-rename-design.sh', '6a78410fd55ad3d6909b679007466e4be13c6bcca3069dc95159fc584f9998ec'),
    ('renamefar', 'tests/ui-rename-far.sh', '74676a5bccb9c60cef4026101a4a1f8c6416e616360c72de6596125079408eed'),
    ('taildrop', 'tests/ui.sh', '3a4348ae855a111f2fe22ca5e114332d60e8c947d969a0d2ffaeb751cc35ab11'),
    ('providers', 'tests/ui-providers.sh', '8882832268a9fd834a6311a4e6f772aea1adbbedb827d13e7d33395bac7bc476'),
    ('grid', 'tests/ui.sh', 'c110e3de413060d05e7bf2f619499915685ec8b183ed07b64cfe79e69387f179'),
    ('columns', 'tests/ui.sh', '76940882999576db993fa0062b18001bb2d9d7d05507d8f4055bf881eb6cbec3'),
    ('columnsbackground', 'tests/ui-columns-background.sh', 'b8393bf7c3e131af2dbaadfba5663e7b1f6f379e5f015c541fa80858066a4708'),
    ('operations', 'tests/ui.sh', '9e8b9781388f81e6c4694a41caf509d497cb9dfb5971501b32b50c62c78a30c0'),
    ('tabs', 'tests/ui.sh', 'a253e4fa31a15f5090c06621112d745985a53b0d723ecb988449ee75a93fca9b'),
    ('tabdrag', 'tests/ui.sh', '9e40bd8fe9c68218e126ab2ee85bb6af9a7694f34cd1c7a22bebe94860c4db90'),
    ('openterminal', 'tests/ui.sh', '6a3095b1ebb8cc8bb582e14068b43c5c446245770c588942e6e26ef4fe5d2ade'),
    ('makeexec', 'tests/ui.sh', '25ef39418dd96383569dcbb301a7b3fc3f0c97d5520d381a33391061a1000d5e'),
    ('renderer', 'tests/ui.sh', '555d48172766af5c2caf79306a0c785c0f239db8d734a0aa784e8fe8ee6c3d1d'),
    ('settings', 'tests/ui.sh', 'a511289d6f952767cb53be97c3b6d4ae1021dab792fed52e72bd7b1673467f75'),
    ('makedefault', 'tests/ui-makedefault.sh', '4f0876bc4ae0f201fceb4eded7e66f5256b618d82e84b48d2f89e975e89e0621'),
    ('scrolllane', 'tests/ui.sh', '6acf486fd4833e2ec6bff1317a78a4213f808054469524e78035bb8bbf83c6d3'),
    ('clickthrough', 'tests/ui.sh', '3a84391666672c31611e914b3fe276cb51ce6accd3b7a3a24622c52d99ad64f6'),
    ('wheelunder', 'tests/ui.sh', '87d31857f8dcf862bcbc5adce9fa6f5f83e091cc2e1b0517fd074985b5ffeadf'),
    ('overlays', 'tests/ui.sh', '300220bf11f5f0fa11deb6db32cf9e8fcaa7058254c559433900fa26fa7e97b6'),
    ('views', 'tests/ui.sh', '9955b61fa9951bdd85497507c3ff6f570d90a02eb6ab439489c098bdb94daf98'),
    ('formats', 'tests/ui.sh', 'faa5218df3cb10998b3e485b18c37f2bc1ffb3951c62969c9c5a66b4af5d011b'),
    ('previewviews', 'tests/ui.sh', '508166e732c16eac1c9ee07e6eafec7d88c413803122b9e8293eaf8c6a3f70be'),
    ('reclick', 'tests/ui.sh', '9ad306b207bb5eb6f4b19da53ea0d30b2b74d12084ff3dc6fd603a487ecc1562'),
    ('colroot', 'tests/ui.sh', '5edc90a01827923bf88f4bd17ca6933807de07f8c6f82906c9c7656b7c8d9ce0'),
    ('colwatch', 'tests/ui-colwatch.sh', 'f456d2c1f2c692d4854f965aa2f8a01e1cecaa671826a53d92bfd1ba0c01dd64'),
    ('hangshare', 'tests/ui.sh', '75b03f3fcc14fa841b7a768b169355619ebbfe727e3b82242c30d1509d39b912'),
    ('hanglisting', 'tests/ui.sh', 'e53a3054cb139d076e093160b677234ae2a6a9d2e2df87a8d1994164d066ef54'),
    ('hanginspect', 'tests/ui.sh', 'e09c460b1e8723c8642b449d92965003df40776f47d4ac8d9e8472851fe2bb99'),
    ('openwithdesign', 'tests/ui-openwith-design.sh', '32044b8d56fccc56f1c645135ba04bc5e47d54e41fef606ac1cb0a7faf301708'),
    ('noblank', 'tests/ui-noblank.sh', '33f6600577fd90efe6da1bd247a450ac33d9eb12caadf2e17ecd2731a6b5c4f0'),
    ('previewswap', 'tests/ui-noblank.sh', 'd37b2b985578a8e4671c1878ac2b911f5f8f3b328abf3ac95c5d20cc65ce5ed0'),
    # norz has close matches; norzqqqq is empty. The six search keys type norzzq.
    ('searchnoblank', 'tests/ui-noblank.sh', '597182d96bfca42e123f0c04a8c097a86f8eaa66d2678647f9ed5dc45f401b50'),
    ('lookopen', 'tests/ui-noblank.sh', '5968e8013a36fc437b137927383aef690250db9798d82d958214bcf33b322df0'),
    ('transferlive', 'tests/ui-transfer-live.sh', 'f120fb42982fb7412c3e9782164ea69bb4eacd7a9d24d3b787ebd5ec817f4ae6'),
    ('recent', 'tests/ui.sh', 'fb2178d32ac897c6110c1102cc035a8523c91eb86ba348f016ddfc79955048e8'),
    ('middleclick', 'tests/ui.sh', 'e1b5401d6e9a82ef4ccb8aaf8e81f878a8f9426d51921906fc165b9dcd4824df'),
    ('opentab', 'tests/ui.sh', 'd3205c08adbab21a3226b80ec0fac83390a5b8e20cef3c8d4eafc10b7de30f03'),
    ('xwundo', 'tests/ui.sh', 'd5450dcc1187caa32fdda941e208a0fd5ac736cc0a8ce765417b2040260af725'),
    ('folderviews', 'tests/ui-settings-layout.sh', 'fd625791cca0249407be2c4f327b9e9b0b76ad2eee47414a5f89df648ba13498'),
    ('hiddenlast', 'tests/ui-dirsortstale.sh', '30401026e3a9a912938d24e1b73228d214bd071c2fe2050dd71a68c8cb16dbc1'),
    ('marqueemargin', 'tests/ui-marquee.sh', '9675c6f32a01a4579e77d996002f10da46f41dbc9613a2beaf4709e18fa84a16'),
    ('relstart', 'tests/ui-noblank.sh', '1fd1ffc51308bdbcbcd21a593b6a2205cd336481e8ab8fb24e6c6ff33d894007'),
    ('userthumbnailer', 'tests/ui-preview-policy.sh', 'f1dd890a6bedecc6ed8611a71829de5792ab94ccfa56739fca4e956980a901ea'),
    ('dualtab', 'tests/ui-railpointer.sh', '412a7de9fefe449163eeba9dddc390286536811332bebed336cee3860ca241ba'),
    ('dualdivider', 'tests/ui-railpointer.sh', '1686be6035d8cfb29e4fd6de0d0b145a9dcf60f9a2e5932e8cd27506fac71114'),
    ('dualfoldersort', 'tests/ui.sh', '38bdd8c942117125253e0e946a3d3e50a391754df59fd636d7e50c47f016e7f6'),
)}
BATCHES = {'tests/ui-uish/common-observers.sh': ['scroll', 'colour', 'lifted', 'settings'],
 'tests/ui-uish/launch.sh': ['select', 'renderer'],
 'tests/ui-uish/state.sh': ['xwsettings', 'xwwatch', 'makedefault'],
 'tests/ui-uish/sizes.sh': ['dirsortstale', 'nosweep', 'hiddenlast', 'hanginspect'],
 'tests/ui-uish/inline-rename.sh': ['clickedge', 'renamefirst', 'renamelife', 'renamefar'],
 'tests/ui-uish/modal-routing.sh': ['collide', 'menu', 'clickthrough', 'wheelunder'],
 'tests/ui-uish/places.sh': ['editplace', 'placemenu', 'rename', 'recent'],
 'tests/ui-uish/network-form.sh': ['network', 'netmark', 'networkauth'],
 'tests/ui-uish/network-service.sh': ['networktimeout', 'gvfs', 'unmount', 'hangshare', 'hanglisting'],
 'tests/ui-uish/devices.sh': ['unmounted', 'optical', 'phones', 'eject', 'poweroff'],
 'tests/ui-uish/providers.sh': ['sharebrowser', 'taildrop', 'providers'],
 'tests/ui-uish/thumbnails.sh': ['thumbs', 'hashcache', 'stale', 'userthumbnailer'],
 'tests/ui-uish/quick-look.sh': ['open', 'preview', 'lookopen', 'relstart'],
 'tests/ui-uish/documents.sh': ['pdffocus', 'columns', 'views', 'formats'],
 'tests/ui-uish/media.sh': ['mute', 'previewviews'],
 'tests/ui-uish/preview-swap.sh': ['watch', 'previewswap'],
 'tests/ui-uish/tabs.sh': ['tabs', 'tabdrag', 'middleclick', 'opentab', 'folderviews'],
 'tests/ui-uish/operations.sh': ['dd', 'ddclick', 'operations', 'transferlive', 'xwundo'],
 'tests/ui-uish/open-with.sh': ['openterminal', 'makeexec', 'openwithdesign'],
 'tests/ui-uish/drag.sh': ['griddrop', 'railpointer', 'dualautohidereveal', 'dualdivider'],
 'tests/ui-uish/menus.sh': ['querypaste', 'runscript', 'background', 'hidden', 'reload', 'focus', 'columnsbackground', 'rows'],
 'tests/ui-uish/backend.sh': ['terminal'],
 'tests/ui-uish/autofit.sh': ['columnautofit'],
 'tests/ui-uish/input.sh': ['shiftscroll', 'overlays']}
FUNCTION = re.compile(r"^case_([A-Za-z0-9_]+)\(\) (?:\{[^\n]*\}|\{\n.*?^\}|\(\n.*?^\))(?=\n|$)", re.M | re.S)
DEFINITION = re.compile(r"^[ \t]*(?:function[ \t]+case_([A-Za-z0-9_]+)(?:[ \t]*\([ \t]*\))?|case_([A-Za-z0-9_]+)[ \t]*\([ \t]*\))[ \t]*[({]", re.M)
WANTED = re.compile(r"wanted=\((cursor [^\n]+)\)")
SOURCE = re.compile(r'^\. "\$repo/(tests/[^"\n]+\.sh)"', re.M)
DECLARATION = re.compile(r"^flea_uish_module (tests/ui-uish/[a-z-]+\.sh) ([a-z0-9_ ]+)$", re.M)


class InventoryError(ValueError):
    pass


def functions(text):
    return [(m[1], m[0]) for m in FUNCTION.finditer(text)]


def wanted(text):
    matches = WANTED.findall(text)
    if len(matches) != 1:
        raise InventoryError("missing or ambiguous default wanted list")
    return matches[0].split()


def oracle_sources(repo):
    pending = ["tests/ui.sh"]
    seen = {}
    while pending:
        path = pending.pop(0)
        if path in seen or path.startswith("tests/ui-uish/"):
            continue
        text = (repo / path).read_text()
        seen[path] = text
        pending.extend(SOURCE.findall(text))
    return seen


def check(repo):
    sources = oracle_sources(repo)
    anchor = ': > "$run_log"\n: > "$flea_log"\nnative_started_ms='
    text = sources["tests/ui.sh"]
    if anchor not in text or hashlib.sha256(text[text.index(anchor):].encode()).hexdigest() != NATIVE_TAIL_SHA256:
        raise InventoryError("native execution, receipts or final checks changed")
    names = wanted(sources["tests/ui.sh"])
    if names != list(ORACLE):
        raise InventoryError("default wanted cases are missing, reordered, duplicated or by-name-only")
    definitions = {name: [] for name in names}
    counts = {name: 0 for name in names}
    for path, text in sources.items():
        for match in DEFINITION.finditer(text):
            name = match[1] or match[2]
            if name in counts:
                counts[name] += 1
        for name, body in functions(text):
            if name in definitions:
                definitions[name].append((path, body))
    for name, entries in definitions.items():
        if len(entries) != 1 or counts[name] != 1:
            raise InventoryError(f"oracle {name} has {len(entries)} definitions")
        path, body = entries[0]
        if path != ORACLE[name]["path"] or hashlib.sha256(body.encode()).hexdigest() != ORACLE[name]["sha256"]:
            raise InventoryError(f"oracle {name} changed at {path}")
    manifest = (repo / "tests/ui-uish/manifest.sh").read_text()
    if DEFINITION.search(manifest):
        raise InventoryError("manifest may not define a GPUI case")
    declarations = DECLARATION.findall(manifest)
    if declarations != [(path, " ".join(names)) for path, names in BATCHES.items()]:
        raise InventoryError("GPUI override declarations changed or duplicated")
    seen = set()
    pending = []
    for path, assigned in BATCHES.items():
        source = repo / path
        if not source.exists() and path == "tests/ui-uish/common-observers.sh":
            pending.append(path)
            continue
        names = [m[1] or m[2] for m in DEFINITION.finditer(source.read_text())]
        if names != assigned:
            raise InventoryError(f"missing, duplicate or undeclared GPUI override in {path}: {names}")
        for name in names:
            if name in seen:
                raise InventoryError(f"second GPUI override for {name}")
            seen.add(name)
    inline = sum(item["path"] == "tests/ui.sh" for item in ORACLE.values())
    return {"base": BASE, "wanted": len(ORACLE), "inline": inline,
            "sourced": len(ORACLE) - inline, "declared_overrides": sum(map(len, BATCHES.values())),
            "installed_overrides": len(seen), "pending_modules": pending}


def headless(repo, arguments):
    if os.environ.get("FLEA_TEST_FRONTEND", "qml") != "gpui":
        raise InventoryError("headless driver requires FLEA_TEST_FRONTEND=gpui")
    check(repo)
    helper = repo / "tests/lib/flea_uish_headless.py"
    if not helper.is_file():
        print("UNSUPPORTED headless: UISH-02 real Window driver is not installed", file=sys.stderr)
        return 2
    print("HEADLESS_RUN frontend=gpui native_input=false compositor=false presented_frames=false", flush=True)
    # UISH-02 owns this protocol endpoint and the real Window lifecycle.
    result = subprocess.run([sys.executable, "-B", str(helper), "run-ui", "--repo", str(repo), "--", *(arguments or list(ORACLE))],
                            capture_output=True, text=True, check=False)
    for stream in (result.stdout, result.stderr):
        if "NATIVE_RUN" in stream or re.search(r"(^|\n).*\bnative PASS\b", stream):
            raise InventoryError("headless driver attempted a native receipt")
    for stream, destination in ((result.stdout, sys.stdout), (result.stderr, sys.stderr)):
        for line in stream.splitlines():
            if line.startswith("PASS "):
                line = "HEADLESS_" + line
            if re.fullmatch(r"\d+ of \d+ checks failed", line):
                line = "HEADLESS " + line
            print(line, file=destination)
    return result.returncode if result.returncode >= 0 else 128 - result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("cases", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        repo = args.repo.resolve()
        if args.headless:
            return headless(repo, args.cases[1:] if args.cases[:1] == ["--"] else args.cases)
        print(json.dumps(check(repo), sort_keys=True))
        return 0
    except (InventoryError, OSError, UnicodeError) as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
