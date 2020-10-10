use crate::exhentai::*;
use crate::utils::*;
use crate::{BOT, CONFIG, DB};
use anyhow::Result;
use telegraph_rs::{html_to_node, Page, Telegraph};
use teloxide::prelude::*;
use v_htmlescape::escape;

pub struct ExLoli {
    exhentai: ExHentai,
    telegraph: Telegraph,
}

impl ExLoli {
    pub async fn new() -> Result<Self> {
        let exhentai = CONFIG.init_exhentai().await?;
        let telegraph = CONFIG.init_telegraph().await?;
        Ok(ExLoli {
            exhentai,
            telegraph,
        })
    }

    /// 根据配置文件自动扫描并上传本子
    pub async fn scan_and_upload(&self) -> Result<()> {
        // 筛选最新本子
        let keyword = &CONFIG.exhentai.keyword;
        let page_limit = CONFIG.exhentai.max_pages;
        let galleries = self.exhentai.search_n_pages(keyword, page_limit).await?;

        // 从后往前爬, 保持顺序
        for gallery in galleries.into_iter().rev() {
            if DB.query_gallery_by_url(&gallery.url).is_ok() {
                continue;
            }
            self.upload_gallery_to_telegram(gallery).await?;
        }

        Ok(())
    }

    /// 上传指定 URL 的画廊
    pub async fn upload_gallery_by_url(&self, url: &str) -> Result<()> {
        let mut gallery = self.exhentai.get_gallery_by_url(url).await?;
        gallery.limit = false;
        self.upload_gallery_to_telegram(gallery).await
    }

    /// 将画廊上传到 telegram
    async fn upload_gallery_to_telegram<'a>(&'a self, gallery: BasicGalleryInfo<'a>) -> Result<()> {
        info!("画廊名称: {}", gallery.title);
        info!("画廊地址: {}", gallery.url);

        let gallery = gallery.into_full_info().await?;

        // 判断是否上传过并且不需要更新
        if let Ok(g) = DB.query_gallery_by_title(&gallery.title) {
            if g.upload_images as usize >= CONFIG.exhentai.max_img_cnt {
                return Err(anyhow::anyhow!("AlreadyUpload"));
            }
        }

        let img_cnt = gallery.get_image_lists().len();
        let img_urls = gallery.upload_images_to_telegraph().await?;

        let overflow = gallery.img_pages.len() != img_cnt;
        let page = self
            .publish_to_telegraph(&gallery.title, &img_urls, overflow)
            .await?;
        info!("文章地址: {}", page.url);

        let message = self.publish_to_telegram(&gallery, &page.url).await?;

        DB.insert_gallery(&gallery, page.url, message.id)
    }

    /// 将画廊内容上传至 telegraph
    async fn publish_to_telegraph<'a>(
        &self,
        title: &str,
        img_urls: &[String],
        overflow: bool,
    ) -> Result<Page> {
        info!("上传到 Telegraph");
        let mut content = img_urls_to_html(&img_urls);
        if overflow {
            content.push_str(r#"<p>图片数量过多, 只显示部分. 完整版请前往 E 站观看.</p>"#);
        }
        self.telegraph
            .create_page(title, &html_to_node(&content), false)
            .await
            .map_err(|e| e.into())
    }

    /// 将画廊内容上传至 telegraph
    async fn publish_to_telegram<'a>(
        &self,
        gallery: &FullGalleryInfo<'a>,
        article: &str,
    ) -> Result<Message> {
        info!("发布到 Telegram 频道");
        let tags = tags_to_string(&gallery.tags);
        let text = format!(
            "{0}\n<code>  预览</code>：<a href=\"{1}\">{2}</a>\n<code>原始地址</code>：<a href=\"{3}\">{3}</a>",
            tags,
            article,
            escape(&gallery.title),
            gallery.url,
        );
        Ok(BOT
            .send_message(CONFIG.telegram.channel_id.clone(), &text)
            .send()
            .await?)
    }
}
