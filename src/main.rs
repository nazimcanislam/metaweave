use clap::Parser;

mod fetcher;
mod analyzer;
use fetcher::fetch_url;
use analyzer::seo::analyze_seo;
use analyzer::social::analyze_social;
use analyzer::assets::analyze_assets;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "The URL to fetch")]
    url: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("Fetching URL: {}", args.url); 

    let response = fetch_url(&args.url).await;

    if let Err(e) = response {
        eprintln!("Error fetching URL: {}", e);
        return;
    }

    let unwrapped_response = response.unwrap();

    let seo_issues = analyze_seo(&unwrapped_response);
    if seo_issues.is_empty() {
        println!("No SEO issues found.");
    } else {
        println!("SEO Issues:");
        for issue in seo_issues {
            println!("- {}", issue);
        }
    }

    let social_issues = analyze_social(&unwrapped_response);
    if social_issues.is_empty() {
        println!("No social media issues found.");
    } else {
        println!("Social Media Issues:");
        for issue in social_issues {
            println!("- {}", issue);
        }
    }

    let asset_issues = analyze_assets(&args.url).await;
    if asset_issues.is_empty() {
        println!("No asset issues found.");
    } else {
        println!("Asset Issues:");
        for issue in asset_issues {
            println!("- {}", issue);
        }
    }
}
